#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${EMBERLANE_INF2_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
MODEL_PROFILE="${MODEL_PROFILE:-qwen25_15b_inf2_economy}"

eval "$("${ROOT_DIR}/scripts/render-env.py" --profile "${MODEL_PROFILE}")"

export HF_HOME="${HF_HOME:-/opt/emberlane/model-cache}"
export TRANSFORMERS_CACHE="${TRANSFORMERS_CACHE:-${HF_HOME}}"
export NEURON_COMPILED_ARTIFACTS="${NEURON_COMPILED_ARTIFACTS:-/opt/emberlane/neuron-cache}"
export VLLM_TARGET_DEVICE="${VLLM_TARGET_DEVICE:-neuron}"
export NEURON_CC_FLAGS="${NEURON_CC_FLAGS:-}"
case " ${NEURON_CC_FLAGS} " in
  *" --retry_failed_compilation "*) ;;
  *) export NEURON_CC_FLAGS="${NEURON_CC_FLAGS} --retry_failed_compilation" ;;
esac
export NEURON_CONTEXT_LENGTH_BUCKETS="${NEURON_CONTEXT_LENGTH_BUCKETS:-}"
export NEURON_TOKEN_GEN_BUCKETS="${NEURON_TOKEN_GEN_BUCKETS:-}"
export VLLM_USE_V1="${VLLM_USE_V1:-0}"
export VLLM_ATTENTION_BACKEND="${VLLM_ATTENTION_BACKEND:-NEURON}"
mkdir -p "${HF_HOME}" "${TRANSFORMERS_CACHE}" "${NEURON_COMPILED_ARTIFACTS}"

# Detect and activate standard AWS Neuron environments if present
for venv in /opt/aws_neuronx_venv_pytorch* /opt/aws_neuron_venv_pytorch* /home/ubuntu/aws_neuronx_venv_pytorch* /home/ubuntu/aws_neuron_venv_pytorch*; do
  if [[ -d "$venv" && -f "${venv}/bin/activate" ]]; then
    echo "[emberlane] activating Neuron environment: ${venv}"
    source "${venv}/bin/activate"
    break
  fi
done

if ! command -v python3 >/dev/null 2>&1; then
  echo "[emberlane] python3 is required but was not found on PATH" >&2
  exit 1
fi

USE_DOCKER_NEURON="${USE_DOCKER_NEURON:-}"
if [[ -z "${USE_DOCKER_NEURON}" ]]; then
  if [[ "${VLLM_TARGET_DEVICE}" == "neuron" ]]; then
    USE_DOCKER_NEURON="true"
  elif ! command -v vllm >/dev/null 2>&1; then
    USE_DOCKER_NEURON="true"
  fi
fi

if [[ "${USE_DOCKER_NEURON}" == "true" ]]; then
  echo "[emberlane] using the Neuron Docker image for runtime launch"
fi

if [[ -n "${S3_NEURON_ARTIFACTS_URI:-}" ]]; then
  echo "Syncing Neuron artifacts from ${S3_NEURON_ARTIFACTS_URI}"
  aws s3 sync "${S3_NEURON_ARTIFACTS_URI}" "${NEURON_COMPILED_ARTIFACTS}/" || true
fi

VLLM_PORT="${PORT:-8000}"
PROXY_PORT="${RUNTIME_PORT:-8080}"
MODEL_PATH="${MODEL_LOCAL_PATH:-${MODEL_ID}}"
NUM_GPU_BLOCKS_OVERRIDE="${NUM_GPU_BLOCKS_OVERRIDE:-${MAX_NUM_SEQS}}"
VLLM_IMAGE="${VLLM_IMAGE:-public.ecr.aws/neuron/pytorch-inference-vllm-neuronx:0.16.0-neuronx-py312-sdk2.29.1-ubuntu24.04}"

vllm_args=(serve "${MODEL_PATH}"
  --tensor-parallel-size "${TENSOR_PARALLEL_SIZE}"
  --block-size "${BLOCK_SIZE}"
  --max-model-len "${MAX_MODEL_LEN}"
  --max-num-seqs "${MAX_NUM_SEQS}"
  --num-gpu-blocks-override "${NUM_GPU_BLOCKS_OVERRIDE}"
  --no-enable-prefix-caching
  --host 0.0.0.0
  --port "${VLLM_PORT}")
cmd=(vllm "${vllm_args[@]}")

echo "Starting ${RUNTIME} profile ${MODEL_PROFILE}: ${MODEL_ID}"
echo "${cmd[@]}"

start_vllm() {
  if [[ "${USE_DOCKER_NEURON}" != "true" && command -v vllm >/dev/null 2>&1 ]]; then
    "${cmd[@]}" &
    vllm_pid=$!
    return 0
  fi

  if ! command -v docker >/dev/null 2>&1; then
    echo "[emberlane] docker is required for the Neuron fallback path but was not found" >&2
    return 1
  fi

  local devices=()
  local dev
  for dev in /dev/neuron*; do
    if [[ -e "${dev}" ]]; then
      devices+=(--device "${dev}:${dev}")
    fi
  done

  docker stop emberlane-vllm >/dev/null 2>&1 || true
  docker rm emberlane-vllm >/dev/null 2>&1 || true

  docker run --rm --name emberlane-vllm --entrypoint vllm \
    "${devices[@]}" \
    --shm-size=2g \
    -v "${HF_HOME}:${HF_HOME}" \
    -v "${NEURON_COMPILED_ARTIFACTS}:${NEURON_COMPILED_ARTIFACTS}" \
    -e "VLLM_USE_V1=${VLLM_USE_V1}" \
    -e "VLLM_TARGET_DEVICE=${VLLM_TARGET_DEVICE}" \
    -e "VLLM_ATTENTION_BACKEND=${VLLM_ATTENTION_BACKEND}" \
    -e "HF_HOME=${HF_HOME}" \
    -e "TRANSFORMERS_CACHE=${TRANSFORMERS_CACHE}" \
    -e "XDG_CACHE_HOME=${XDG_CACHE_HOME:-${HF_HOME}}" \
    -e "NEURON_COMPILED_ARTIFACTS=${NEURON_COMPILED_ARTIFACTS}" \
    -e "HF_TOKEN=${HF_TOKEN:-}" \
    -p "${VLLM_PORT}:8000" \
    "${VLLM_IMAGE}" \
    "${vllm_args[@]}" &
  vllm_pid=$!
}

start_vllm
vllm_pid="${vllm_pid:-}"

cleanup() {
  if kill -0 "${vllm_pid}" >/dev/null 2>&1; then
    kill "${vllm_pid}" >/dev/null 2>&1 || true
    wait "${vllm_pid}" >/dev/null 2>&1 || true
  fi
  if [[ "${SYNC_ARTIFACTS_BACK:-false}" == "true" && -n "${S3_NEURON_ARTIFACTS_URI:-}" ]]; then
    echo "Syncing Neuron artifacts back to ${S3_NEURON_ARTIFACTS_URI}"
    aws s3 sync "${NEURON_COMPILED_ARTIFACTS}/" "${S3_NEURON_ARTIFACTS_URI}" || true
  fi
}

trap cleanup EXIT INT TERM

echo "[emberlane] starting proxy on port ${PROXY_PORT} for upstream ${VLLM_PORT}"
UPSTREAM_BASE_URL="http://127.0.0.1:${VLLM_PORT}" RUNTIME_PORT="${PROXY_PORT}" python3 "${ROOT_DIR}/server/health_proxy.py"

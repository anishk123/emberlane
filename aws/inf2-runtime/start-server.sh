#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${EMBERLANE_INF2_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
MODEL_PROFILE="${MODEL_PROFILE:-llama32_1b}"

eval "$("${ROOT_DIR}/scripts/render-env.py" --profile "${MODEL_PROFILE}")"

export HF_HOME="${HF_HOME:-/opt/emberlane/model-cache}"
export TRANSFORMERS_CACHE="${TRANSFORMERS_CACHE:-${HF_HOME}}"
export NEURON_COMPILED_ARTIFACTS="${NEURON_COMPILED_ARTIFACTS:-/opt/emberlane/neuron-cache}"
export NEURON_CC_FLAGS="${NEURON_CC_FLAGS:-}"
export NEURON_CONTEXT_LENGTH_BUCKETS="${NEURON_CONTEXT_LENGTH_BUCKETS:-}"
export NEURON_TOKEN_GEN_BUCKETS="${NEURON_TOKEN_GEN_BUCKETS:-}"
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

if ! command -v vllm >/dev/null 2>&1; then
  echo "[emberlane] vllm is required but was not found on PATH" >&2
  echo "[emberlane] install the Neuron/vLLM runtime or boot from the validated baked AMI" >&2
  exit 1
fi

if [[ -n "${S3_NEURON_ARTIFACTS_URI:-}" ]]; then
  echo "Syncing Neuron artifacts from ${S3_NEURON_ARTIFACTS_URI}"
  aws s3 sync "${S3_NEURON_ARTIFACTS_URI}" "${NEURON_COMPILED_ARTIFACTS}/" || true
fi

VLLM_PORT="${PORT:-8000}"
PROXY_PORT="${RUNTIME_PORT:-8080}"

cmd=(
  vllm serve "${MODEL_ID}"
  --device neuron
  --tensor-parallel-size "${TENSOR_PARALLEL_SIZE}"
  --block-size "${BLOCK_SIZE}"
  --max-model-len "${MAX_MODEL_LEN}"
  --max-num-seqs "${MAX_NUM_SEQS}"
  --host 0.0.0.0
  --port "${VLLM_PORT}"
)

echo "Starting ${RUNTIME} profile ${MODEL_PROFILE}: ${MODEL_ID}"
echo "${cmd[@]}"
"${cmd[@]}" &
vllm_pid=$!

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

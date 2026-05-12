#!/usr/bin/env bash
set -euo pipefail

LOG=/var/log/emberlane/bootstrap.log
mkdir -p /opt/emberlane/model-cache /opt/emberlane/neuron-cache /var/log/emberlane /etc/emberlane
exec > >(tee -a "${LOG}") 2>&1

echo "Emberlane Inf2 bootstrap starting at $(date -Is)"

export PATH="/opt/aws/neuron/bin:/opt/aws/neuronx/bin:${PATH:-}"

if [[ ! -e /dev/neuron0 ]]; then
  echo "ERROR: /dev/neuron0 not found. Use an Inf2 instance with Neuron drivers/runtime installed." >&2
  exit 1
fi

if command -v neuron-ls >/dev/null 2>&1; then
  neuron-ls || true
else
  echo "neuron-ls not found; continuing because some base AMIs omit it from PATH."
fi

ROOT_DIR="${EMBERLANE_INF2_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
INF2_VENV="${INF2_VENV:-/opt/emberlane/inf2-venv}"
install -m 0644 "${ROOT_DIR}/systemd/emberlane-inf2.service" /etc/systemd/system/emberlane-inf2.service

ensure_inf2_python() {
  if [[ ! -x "${INF2_VENV}/bin/python" ]]; then
    echo "Creating isolated Python virtualenv at ${INF2_VENV}"
    if ! python3 -m venv "${INF2_VENV}"; then
      if command -v apt-get >/dev/null 2>&1; then
        echo "python3 -m venv is unavailable; installing python3-venv"
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -y
        apt-get install -y python3-venv
        python3 -m venv "${INF2_VENV}"
      else
        echo "ERROR: python3 -m venv failed and apt-get is unavailable" >&2
        exit 1
      fi
    fi
  fi

  if ! "${INF2_VENV}/bin/python" - <<'PY' >/dev/null 2>&1
import importlib.util
raise SystemExit(0 if importlib.util.find_spec("huggingface_hub") else 1)
PY
  then
    echo "Installing huggingface_hub into ${INF2_VENV}"
    "${INF2_VENV}/bin/python" -m pip install --quiet --upgrade "huggingface_hub>=0.23.0"
  fi
}

if ! command -v python3 >/dev/null 2>&1; then
  echo "ERROR: python3 is required for Inf2 bootstrap" >&2
  exit 1
fi

ensure_inf2_python
export INF2_PYTHON="${INF2_VENV}/bin/python"

if [[ ! -f /etc/emberlane/inf2.env ]]; then
  cat >/etc/emberlane/inf2.env <<'ENV'
MODEL_PROFILE=qwen3_4b_inf2_4k
HF_HOME=/opt/emberlane/model-cache
TRANSFORMERS_CACHE=/opt/emberlane/model-cache
NEURON_COMPILED_ARTIFACTS=/opt/emberlane/neuron-cache
ENV
fi

set -a
source /etc/emberlane/inf2.env
set +a

if [[ -n "${S3_NEURON_ARTIFACTS_URI:-}" ]]; then
  echo "Pre-syncing Neuron compiled artifacts from ${S3_NEURON_ARTIFACTS_URI}"
  aws s3 sync "${S3_NEURON_ARTIFACTS_URI}" "${NEURON_COMPILED_ARTIFACTS:-/opt/emberlane/neuron-cache}/" || true
fi

if [[ -n "${HF_TOKEN:-}" || -n "${MODEL_ID:-}" ]]; then
  MODEL_LOCAL_PATH="$("${ROOT_DIR}/scripts/download-model.sh")"
  if [[ -n "${MODEL_LOCAL_PATH:-}" ]]; then
    printf '\nMODEL_LOCAL_PATH=%s\n' "${MODEL_LOCAL_PATH}" >> /etc/emberlane/inf2.env
    export MODEL_LOCAL_PATH
  fi
fi

systemctl daemon-reload
systemctl enable emberlane-inf2.service
systemctl restart emberlane-inf2.service
systemctl --no-pager status emberlane-inf2.service || true

echo "Emberlane Inf2 bootstrap complete at $(date -Is)"

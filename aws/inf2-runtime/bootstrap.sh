#!/usr/bin/env bash
set -euo pipefail

LOG=/var/log/emberlane/bootstrap.log
mkdir -p /opt/emberlane/model-cache /opt/emberlane/neuron-cache /var/log/emberlane /etc/emberlane
exec > >(tee -a "${LOG}") 2>&1

echo "Emberlane Inf2 bootstrap starting at $(date -Is)"

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
install -m 0644 "${ROOT_DIR}/systemd/emberlane-inf2.service" /etc/systemd/system/emberlane-inf2.service

if [[ ! -f /etc/emberlane/inf2.env ]]; then
  cat >/etc/emberlane/inf2.env <<'ENV'
MODEL_PROFILE=llama32_1b
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
  "${ROOT_DIR}/scripts/download-model.sh" || true
fi

systemctl daemon-reload
systemctl enable emberlane-inf2.service
systemctl restart emberlane-inf2.service
systemctl --no-pager status emberlane-inf2.service || true

echo "Emberlane Inf2 bootstrap complete at $(date -Is)"

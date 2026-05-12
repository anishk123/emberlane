#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${EMBERLANE_INF2_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
MODEL_PROFILE="${MODEL_PROFILE:-qwen3_4b_inf2_4k}"
export NEURON_COMPILED_ARTIFACTS="${NEURON_COMPILED_ARTIFACTS:-/opt/emberlane/neuron-cache}"
mkdir -p "${NEURON_COMPILED_ARTIFACTS}"

echo "Starting a short compile warmup for MODEL_PROFILE=${MODEL_PROFILE}"
"${ROOT_DIR}/start-server.sh" &
pid=$!
trap 'kill ${pid} >/dev/null 2>&1 || true' EXIT

"${ROOT_DIR}/scripts/smoke-test.sh" --wait

if [[ "${SYNC_ARTIFACTS_BACK:-false}" == "true" && -n "${S3_NEURON_ARTIFACTS_URI:-}" ]]; then
  aws s3 sync "${NEURON_COMPILED_ARTIFACTS}/" "${S3_NEURON_ARTIFACTS_URI}"
fi

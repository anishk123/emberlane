#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${EMBERLANE_INF2_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
MODEL_PROFILE="${MODEL_PROFILE:-qwen3_4b_inf2_4k}"
eval "$("${ROOT_DIR}/scripts/render-env.py" --profile "${MODEL_PROFILE}")"

export HF_HOME="${HF_HOME:-/opt/emberlane/model-cache}"
export TRANSFORMERS_CACHE="${TRANSFORMERS_CACHE:-${HF_HOME}}"
mkdir -p "${HF_HOME}"

if ! python3 - <<'PY' >/dev/null 2>&1
import importlib.util
raise SystemExit(0 if importlib.util.find_spec("huggingface_hub") else 1)
PY
then
  python3 -m pip install --quiet --upgrade --break-system-packages "huggingface_hub>=0.23.0"
fi

if [[ -z "${HF_TOKEN:-}" ]]; then
  echo "HF_TOKEN is not set. Public models may still download; gated models require a token." >&2
fi

python3 - <<'PY'
import os
from huggingface_hub import snapshot_download

path = snapshot_download(
    repo_id=os.environ["MODEL_ID"],
    cache_dir=os.environ.get("HF_HOME", "/opt/emberlane/model-cache"),
    token=os.environ.get("HF_TOKEN") or None,
)
print(path)
PY

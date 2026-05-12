#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${EMBERLANE_INF2_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
MODEL_PROFILE="${MODEL_PROFILE:-qwen3_4b_inf2_4k}"
INF2_VENV="${INF2_VENV:-/opt/emberlane/inf2-venv}"
eval "$("${ROOT_DIR}/scripts/render-env.py" --profile "${MODEL_PROFILE}")"

export HF_HOME="${HF_HOME:-/opt/emberlane/model-cache}"
export TRANSFORMERS_CACHE="${TRANSFORMERS_CACHE:-${HF_HOME}}"
mkdir -p "${HF_HOME}"

PYTHON_BIN="${INF2_PYTHON:-${INF2_VENV}/bin/python}"

if [[ ! -x "${PYTHON_BIN}" ]]; then
  if ! command -v python3 >/dev/null 2>&1; then
    echo "python3 is required to download the Inf2 model" >&2
    exit 1
  fi
  PYTHON_BIN="python3"
fi

if ! "${PYTHON_BIN}" - <<'PY' >/dev/null 2>&1
import importlib.util
raise SystemExit(0 if importlib.util.find_spec("huggingface_hub") else 1)
PY
then
  if [[ "${PYTHON_BIN}" == "python3" ]]; then
    echo "huggingface_hub is missing and no Inf2 virtualenv was found" >&2
    echo "Run the Inf2 bootstrap first so it can create /opt/emberlane/inf2-venv" >&2
    exit 1
  fi
  "${PYTHON_BIN}" -m pip install --quiet --upgrade "huggingface_hub>=0.23.0"
fi

if [[ -z "${HF_TOKEN:-}" ]]; then
  echo "HF_TOKEN is not set. Public models may still download; gated models require a token." >&2
fi

"${PYTHON_BIN}" - <<'PY'
import os
from huggingface_hub import snapshot_download

path = snapshot_download(
    repo_id=os.environ["MODEL_ID"],
    cache_dir=os.environ.get("HF_HOME", "/opt/emberlane/model-cache"),
    token=os.environ.get("HF_TOKEN") or None,
)
print(path)
PY

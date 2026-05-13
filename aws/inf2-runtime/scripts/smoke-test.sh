#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080}"
WAIT=false
if [[ "${1:-}" == "--wait" ]]; then
  WAIT=true
fi

if [[ "${WAIT}" == "true" ]]; then
  deadline=$((SECONDS + 900))
  until curl -fsS "${BASE_URL}/health" >/dev/null; do
    if (( SECONDS > deadline )); then
      echo "Timed out waiting for ${BASE_URL}/health" >&2
      exit 1
    fi
    sleep 5
  done
fi

curl -fsS "${BASE_URL}/health"
curl -fsS "${BASE_URL}/v1/models"
curl -fsS -X POST "${BASE_URL}/v1/chat/completions" \
  -H "Content-Type: application/json" \
  -d "{\"model\":\"${MODEL_ID:-Qwen/Qwen2.5-1.5B-Instruct}\",\"messages\":[{\"role\":\"user\",\"content\":\"Say hello from Emberlane Inf2.\"}],\"stream\":false}"

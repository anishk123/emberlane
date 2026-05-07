#!/usr/bin/env bash
set -euo pipefail

URL="${1:-${LAMBDA_URL:-}}"
MODEL="${MODEL:-meta-llama/Llama-3.2-1B}"
TIMEOUT_SECS="${TIMEOUT_SECS:-900}"
RETRY_SECS="${RETRY_SECS:-10}"

if [[ -z "$URL" ]]; then
  echo "usage: LAMBDA_URL=https://... $0 [lambda_url]" >&2
  exit 1
fi

headers=(-H "Content-Type: application/json")
if [[ -n "${API_KEY:-}" ]]; then
  headers+=(-H "Authorization: Bearer ${API_KEY}")
fi

payload="$(python3 - <<PY
import json, os
print(json.dumps({
  "model": os.environ.get("MODEL", "$MODEL"),
  "messages": [{"role": "user", "content": "Say hello from Emberlane."}],
  "stream": False
}))
PY
)"

deadline=$((SECONDS + TIMEOUT_SECS))
while true; do
  tmp="$(mktemp)"
  status="$(curl -sS -o "$tmp" -w "%{http_code}" -X POST "${URL%/}/v1/chat/completions" "${headers[@]}" -d "$payload" || true)"
  body="$(cat "$tmp")"
  rm -f "$tmp"

  echo "HTTP $status"
  echo "$body"

  if [[ "$status" != "202" ]]; then
    [[ "$status" =~ ^2 ]] && exit 0
    exit 1
  fi

  if (( SECONDS >= deadline )); then
    echo "timed out waiting for runtime to warm" >&2
    exit 1
  fi

  echo "runtime warming; retrying in ${RETRY_SECS}s"
  sleep "$RETRY_SECS"
done

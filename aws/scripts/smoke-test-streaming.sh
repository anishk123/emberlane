#!/usr/bin/env bash
set -euo pipefail

URL="${1:-${LAMBDA_URL:-}}"
MODEL="${MODEL:-meta-llama/Llama-3.2-1B}"

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
  "messages": [{"role": "user", "content": "Stream one short sentence."}],
  "stream": True
}))
PY
)"

echo "Streaming request to ${URL%/}/v1/chat/completions"
response="$(mktemp)"
status="$(curl -N -sS -o "$response" -w "%{http_code}" -X POST "${URL%/}/v1/chat/completions" "${headers[@]}" -d "$payload" || true)"
cat "$response"
rm -f "$response"
echo
echo "HTTP $status"

if [[ "$status" == "400" ]]; then
  echo "Streaming may be unsupported by this bridge/networking mode. Use buffered smoke-test-lambda.sh as a fallback."
fi

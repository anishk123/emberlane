#!/usr/bin/env bash
set -euo pipefail

URL="${1:-${HEALTH_URL:-}}"
TIMEOUT_SECS="${TIMEOUT_SECS:-900}"
RETRY_SECS="${RETRY_SECS:-10}"

if [[ -z "$URL" ]]; then
  echo "usage: HEALTH_URL=http://alb/health $0 [health_url]" >&2
  exit 1
fi

deadline=$((SECONDS + TIMEOUT_SECS))
while true; do
  status="$(curl -sS -o /dev/null -w "%{http_code}" "$URL" || true)"
  echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $URL -> HTTP $status"
  [[ "$status" == "200" ]] && exit 0
  if (( SECONDS >= deadline )); then
    echo "timed out waiting for health" >&2
    exit 1
  fi
  sleep "$RETRY_SECS"
done

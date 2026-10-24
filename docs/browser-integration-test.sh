#!/usr/bin/env sh
# Manual smoke-test helper. Usage: ./docs/browser-integration-test.sh PORT TOKEN [PAIRED_EXTENSION_ORIGIN]
set -eu
PORT=${1:?pass local endpoint port}
TOKEN=${2:?pass token from the app's private browser-connection.json}
ORIGIN=${3:-}
BASE="http://127.0.0.1:${PORT}"

printf '1) No token must be rejected (401):\n'
curl -sS -o /dev/null -w '%{http_code}\n' "$BASE/v1/ping"
printf '2) Wrong token must be rejected (401):\n'
curl -sS -o /dev/null -w '%{http_code}\n' -H 'Authorization: Bearer wrong' "$BASE/v1/ping"
printf '3) Correct token must work (200):\n'
curl -sS -o /dev/null -w '%{http_code}\n' -H "Authorization: Bearer $TOKEN" "$BASE/v1/ping"
if [ -n "$ORIGIN" ]; then
  printf '4) Wrong origin must be rejected (403):\n'
  curl -sS -o /dev/null -w '%{http_code}\n' -H "Authorization: Bearer $TOKEN" -H 'Origin: https://unexpected.invalid' "$BASE/v1/ping"
fi
printf '5) file URL must be rejected (400):\n'
curl -sS -o /dev/null -w '%{http_code}\n' -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{"url":"file:///etc/passwd"}' "$BASE/v1/download"
printf '6) Oversized body must be rejected (413):\n'
python3 -c 'print("x" * (1024 * 1024 + 1), end="")' | curl -sS -o /dev/null -w '%{http_code}\n' -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' --data-binary @- "$BASE/v1/download"

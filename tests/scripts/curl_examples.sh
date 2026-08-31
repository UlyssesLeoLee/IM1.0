#!/usr/bin/env bash
# ============================================================================
# curl_examples.sh — commented REST endpoint probes.
#
# Platform: WSL / Git Bash / macOS / Linux.
#          NOT PowerShell (see tests/scripts/*.ps1 for Windows).
#          On Windows use:
#            $env:TOKEN="..."; Invoke-RestMethod -Uri http://127.0.0.1:18080/v1/auth/guest ...
#
# Prereqs (manual install — script does NOT auto-install):
#   - curl 7.79+
#   - jq 1.6+
#   - im-gateway running locally on port 18080
#     (tests/scripts/start_im_gateway_mock.ps1 -WaitForReady)
#
# Usage:
#   bash tests/scripts/curl_examples.sh
#   # or override host/port:
#   API_BASE=http://127.0.0.1:18080 bash tests/scripts/curl_examples.sh
#
# Reference: docs/templates/04-detailed-design/auxiliary/
#            aux-13-protocol-frame-samples.md v1.1.0 §3
# ============================================================================

set -euo pipefail

API_BASE="${API_BASE:-http://127.0.0.1:18080}"
ENV_ID="${ENV_ID:-7c9e6679-7425-40de-944b-e07fc1f90ae7}"
CONV_ID="${CONV_ID:-7c9e6679-7425-40de-944b-e07fc1f90ae7}"
USER_ID="${USER_ID:-1a2e6679-7425-40de-944b-e07fc1f90ae7}"

# ---- Pre-flight ------------------------------------------------------------
if ! command -v curl >/dev/null 2>&1; then
  echo "ERROR: curl not on PATH." >&2; exit 127
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "ERROR: jq not on PATH." >&2; exit 127
fi

req_id=$(uuidgen 2>/dev/null || python -c 'import uuid;print(uuid.uuid4())')

echo "==> [1/5] POST /v1/auth/guest (login)"
LOGIN_RES=$(curl -sS -X POST "$API_BASE/v1/auth/guest" \
  -H "Content-Type: application/json" \
  -d "{\"environment_id\":\"$ENV_ID\"}")
echo "$LOGIN_RES" | jq .
TOKEN=$(echo "$LOGIN_RES" | jq -r .access_token)

echo "==> [2/5] POST /v1/auth/refresh"
curl -sS -X POST "$API_BASE/v1/auth/refresh" \
  -H "Content-Type: application/json" \
  -d '{"refresh_token":"rt_9b8e6679-7425-40de-944b-e07fc1f90ae7"}' | jq .

echo "==> [3/5] POST /v1/auth/logout"
curl -sS -o /dev/null -w "HTTP %{http_code}\n" -X POST "$API_BASE/v1/auth/logout" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "{\"refresh_token\":\"rt_9b8e6679-7425-40de-944b-e07fc1f90ae7\",\"everywhere\":false}"

echo "==> [4/5] GET /v1/users/{id}"
curl -sS "$API_BASE/v1/users/$USER_ID" \
  -H "Authorization: Bearer $TOKEN" | jq .

echo "==> [5/5] POST /v1/conversations/{id}/messages"
curl -sS -X POST "$API_BASE/v1/conversations/$CONV_ID/messages" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -H "X-IM-Idempotency-Key: $req_id" \
  -d '{"kind":"text","content":{"text":"hello from curl"},"reply_to":null}' | jq .

echo "==> DONE"

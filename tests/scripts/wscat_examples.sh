#!/usr/bin/env bash
# ============================================================================
# wscat_examples.sh — commented WS endpoint probes via wscat.
#
# Platform: WSL / Git Bash / macOS / Linux.
#          NOT PowerShell (see tests/scripts/*.ps1 for Windows).
#          On Windows use:  wscat -c ws://127.0.0.1:18081/ws
#
# Prereqs (manual install — script does NOT auto-install):
#   - Node.js 18+ on PATH
#   - wscat globally:   npm i -g wscat
#   - im-gateway running locally on port 18081
#     (tests/scripts/start_im_gateway_mock.ps1 -WaitForReady)
#
# Usage:
#   TOKEN=$(jq -r .access_token tests/data/rest/login_response.json)
#   CONV_ID=$(jq -r .id tests/data/rest/.../conversation.json)   # see REST
#   bash tests/scripts/wscat_examples.sh "$TOKEN" "$CONV_ID"
#
# What this script does:
#   - Demonstrates the canonical WS handshake (auth → connected).
#   - Sends ping, send_message, typing, mark_read frames.
#   - Reads the matching ack frames.
#   - Stops after 3s idle (so the script actually exits).
#
# Reference: docs/templates/04-detailed-design/auxiliary/
#            aux-13-protocol-frame-samples.md v1.1.0 §1
# ============================================================================

set -euo pipefail

TOKEN="${1:-$(jq -r .access_token tests/data/rest/login_response.json)}"
CONV_ID="${2:-7c9e6679-7425-40de-944b-e07fc1f90ae7}"
WS_URL="${WS_URL:-ws://127.0.0.1:18081/ws}"

# ---- Pre-flight checks (do NOT install; fail loudly if missing) -------------
if ! command -v wscat >/dev/null 2>&1; then
  echo "ERROR: wscat not on PATH. Install manually:  npm i -g wscat" >&2
  exit 127
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "ERROR: jq not on PATH. Install manually via your OS package manager." >&2
  exit 127
fi

echo "==> Connecting to $WS_URL"
echo "==> Token: ${TOKEN:0:20}..."
echo "==> Conversation: $CONV_ID"

# ---- Send frames via heredoc; wscat takes JSON lines on stdin ---------------
{
  # 1) auth
  jq -n --arg t "$TOKEN" '{type:"auth", req_id:"11111111-1111-4111-8111-111111111111", access_token:$t}'
  sleep 0.3
  # 2) ping (no req_id)
  jq -n '{type:"ping", ts: 1692528000000}'
  sleep 0.3
  # 3) send_message
  jq -n --arg c "$CONV_ID" '{
    type:"send_message",
    req_id:"22222222-2222-4222-8222-222222222222",
    conversation_id:$c,
    idempotency_key:"33333333-3333-4333-8333-333333333333",
    kind:"text",
    content:{text:"hi from wscat"},
    reply_to: null
  }'
  sleep 0.3
  # 4) typing
  jq -n --arg c "$CONV_ID" '{
    type:"typing", req_id:"88888888-8888-4888-8888-888888888888", conversation_id:$c
  }'
  sleep 0.3
  # 5) mark_read
  jq -n --arg c "$CONV_ID" '{
    type:"mark_read", req_id:"77777777-7777-4777-8777-777777777777",
    conversation_id:$c, sequence: 1
  }'
  sleep 3
  # explicit exit (wscat closes stdin to disconnect)
} | wscat -c "$WS_URL"

echo "==> wscat session ended"

#!/bin/bash
set -e

# e2e/suites/relay/cases/02_reject_invalid_pvs.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/relay/docker-compose-relay.yaml"
PORT_RELAY=8284
CASE_TMP=$(ensure_tmp_dir "relay_02")

cleanup() {
    stop_pid "$CASE_TMP/relay.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Config
mkdir -p "$CASE_TMP/relay_storage"
echo "{}" > "$CASE_TMP/relay_input.yaml"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-reject
http:
  bind: "127.0.0.1:$PORT_RELAY"
storage:
  root_dir: "$CASE_TMP/relay_storage"
artifact:
  lkg_path: "$CASE_TMP/relay_storage/lkg.pvs"
pipeline:
  ingest:
    source:
      kind: file
      path: "$CASE_TMP/relay_input.yaml"
EOFCONFIG

# 2. Start Relay
RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 3. Get start version
START_VERSION=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

# 4. Attempt to publish corrupted PVS bytes
HTTP_CODE=$(curl -s -w "%{http_code}" -o /dev/null -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 100" \
    --data-binary "not-a-pvs")

# Verify request failed (should get 4xx or 5xx)
if [[ "$HTTP_CODE" -lt 400 ]]; then
    echo "Expected error response, got HTTP $HTTP_CODE"
    exit 1
fi

# 5. Verify version didn't increment
CURRENT_VERSION=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)
if [[ "$CURRENT_VERSION" != "$START_VERSION" ]]; then
    echo "Version should not have incremented: was $START_VERSION, now $CURRENT_VERSION"
    exit 1
fi

echo "✅ Case 02_reject_invalid_pvs passed"

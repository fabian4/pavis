#!/bin/bash
set -e

# e2e/suites/relay/cases/03_long_poll_semantics.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/relay/docker-compose-relay.yaml"
PORT_RELAY=8285
CASE_TMP=$(ensure_tmp_dir "relay_03")

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
  name: pavis-relay-longpoll
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

# 3. Get current version
v_start=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

# 4. Start long-poll request in background (2 second timeout)
(curl -s -w "%{http_code}" -H "X-Pavis-Version: $v_start" \
  "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=2000" \
  -o "$CASE_TMP/longpoll_response.pvs" > "$CASE_TMP/longpoll_status.txt") &
LONGPOLL_PID=$!

# 5. Wait 500ms then publish new config
sleep 0.5

CONFIG_YAML="$CASE_TMP/test.yaml"
cat <<EOFYAML > "$CONFIG_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:8080"
upstreams: []
routes: []
EOFYAML

CONFIG_PVS="$CASE_TMP/test.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: $((v_start + 1))" \
    --data-binary "@$CONFIG_PVS"

# 6. Wait for long-poll to complete (should be quick, not full 2s timeout)
wait $LONGPOLL_PID 2>/dev/null || true

# 7. Verify long-poll returned quickly with new version
HTTP_STATUS=$(cat "$CASE_TMP/longpoll_status.txt")
if [ "$HTTP_STATUS" == "200" ]; then
    echo "✅ Case 03_long_poll_semantics passed"
else
    echo "❌ Expected 200, got $HTTP_STATUS"
    exit 1
fi

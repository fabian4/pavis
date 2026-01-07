#!/bin/bash
set -e

# e2e/suites/relay/cases/05_observability.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/relay/docker-compose-relay.yaml"
PORT_RELAY=8287
CASE_TMP=$(ensure_tmp_dir "relay_05")

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
  name: pavis-relay-observability
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

# 3. Check status endpoint has checksum
STATUS=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status")
if [[ "$STATUS" != *"checksum"* ]]; then
    echo "❌ Status should include checksum field"
    exit 1
fi

# 4. Get metrics before failure
METRICS_BEFORE=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/metrics")

# 5. Publish invalid PVS to trigger failure
curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 100" \
    --data-binary "not-a-pvs" >/dev/null 2>&1 || true

sleep 0.5

# 6. Get metrics after failure
METRICS_AFTER=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/metrics")

# 7. Check that publish_fail_total increased
FAIL_BEFORE=$(echo "$METRICS_BEFORE" | grep "pavis_relay_publish_fail_total" | awk '{print $2}' || echo "0")
FAIL_AFTER=$(echo "$METRICS_AFTER" | grep "pavis_relay_publish_fail_total" | awk '{print $2}' || echo "0")

if [ "$FAIL_AFTER" -gt "$FAIL_BEFORE" ]; then
    echo "✅ Case 05_observability passed"
else
    echo "❌ Publish fail metric did not increment"
    exit 1
fi

#!/bin/bash
set -e

# e2e/suites/relay/cases/07_persistence_recovery.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/relay/docker-compose-relay.yaml"
PORT_RELAY=8289
CASE_TMP=$(ensure_tmp_dir "relay_07")

cleanup() {
    stop_pid "$CASE_TMP/relay.pid"
    stop_pid "$CASE_TMP/relay2.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# Skip in Docker mode
if [ "${TEST_MODE:-binary}" == "docker" ]; then
    echo "⏭️  Skipping 07_persistence_recovery (Docker mode)"
    exit 0
fi

# 1. Config
mkdir -p "$CASE_TMP/relay_storage"
echo "{}" > "$CASE_TMP/relay_input.yaml"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-recovery
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

# 3. Publish config
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
    -H "x-pavis-version: 1" \
    --data-binary "@$CONFIG_PVS"

# Wait for LKG to be written
sleep 1

# Verify LKG file exists
if [ ! -f "$CASE_TMP/relay_storage/lkg.pvs" ]; then
    echo "❌ LKG file was not created"
    exit 1
fi

# 4. Stop relay
stop_pid "$CASE_TMP/relay.pid"
sleep 1

# 5. Start new relay instance with same config
RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay2.log" 2>&1 &
echo $! > "$CASE_TMP/relay2.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 6. Verify new instance loaded LKG (version should be 1)
v_recovered=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

if [ "$v_recovered" -eq "1" ]; then
    echo "✅ Case 07_persistence_recovery passed"
else
    echo "❌ Expected version 1 after recovery, got $v_recovered"
    exit 1
fi

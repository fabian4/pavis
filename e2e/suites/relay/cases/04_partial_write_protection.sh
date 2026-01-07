#!/bin/bash
set -e

# e2e/suites/relay/cases/04_partial_write_protection.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/relay/docker-compose-relay.yaml"
PORT_RELAY=8286
CASE_TMP=$(ensure_tmp_dir "relay_04")

cleanup() {
    # Restore permissions
    chmod -R 755 "$CASE_TMP/relay_storage" 2>/dev/null || true
    stop_pid "$CASE_TMP/relay.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# Skip in Docker mode
if [ "${TEST_MODE:-binary}" == "docker" ]; then
    echo "⏭️  Skipping 04_partial_write_protection (Docker mode)"
    exit 0
fi

# 1. Config
mkdir -p "$CASE_TMP/relay_storage"
echo "{}" > "$CASE_TMP/relay_input.yaml"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-partial
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

# 3. Publish initial config successfully
CONFIG_YAML="$CASE_TMP/config1.yaml"
cat <<EOFYAML > "$CONFIG_YAML"
listeners:
  - name: "v1"
    address: "127.0.0.1:8080"
upstreams: []
routes: []
EOFYAML

CONFIG_PVS="$CASE_TMP/config1.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$CONFIG_PVS"

sleep 1
v_before=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

# 4. Block LKG path by replacing file with directory
rm -f "$CASE_TMP/relay_storage/lkg.pvs"
mkdir -p "$CASE_TMP/relay_storage/lkg.pvs"

# 5. Try to publish - should fail
CONFIG_YAML2="$CASE_TMP/config2.yaml"
cat <<EOFYAML > "$CONFIG_YAML2"
listeners:
  - name: "v2"
    address: "127.0.0.1:8081"
upstreams: []
routes: []
EOFYAML

CONFIG_PVS2="$CASE_TMP/config2.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML2" "$CONFIG_PVS2"

HTTP_CODE=$(curl -s -w "%{http_code}" -o /dev/null -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$CONFIG_PVS2")

sleep 1

# 6. Verify version didn't change and LKG is still a directory
v_after=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

if [ "$v_after" -eq "$v_before" ] && [ -d "$CASE_TMP/relay_storage/lkg.pvs" ]; then
    echo "✅ Case 04_partial_write_protection passed"
else
    echo "❌ Version changed or LKG was updated despite write failure"
    exit 1
fi

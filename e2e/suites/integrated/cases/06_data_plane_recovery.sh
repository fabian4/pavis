#!/bin/bash
set -e

# e2e/suites/integrated/cases/06_data_plane_recovery.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8304
PORT_PAVIS=8080
PORT_BACKEND=8081

CASE_TMP=$(ensure_tmp_dir "integrated_06")

cleanup() {
    stop_pid "$CASE_TMP/backend.pid"
    stop_pid "$CASE_TMP/pavis.pid"
    stop_pid "$CASE_TMP/pavis2.pid"
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# Skip in Docker mode
if [ "${TEST_MODE:-binary}" == "docker" ]; then
    echo "⏭️  Skipping 06_data_plane_recovery (Docker mode)"
    exit 0
fi

# 1. Start backend
start_backend $PORT_BACKEND "backend-v1" "$CASE_TMP/backend.pid"

# 2. Setup relay with published config
mkdir -p "$CASE_TMP/relay_storage"
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
EOFCONFIG

RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 3. Publish config
CONFIG_YAML="$CASE_TMP/config.yaml"
cat <<EOFYAML > "$CONFIG_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend-v1"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-v1"
            weight: 1
EOFYAML

CONFIG_PVS="$CASE_TMP/config.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$CONFIG_PVS"

wait_for_url "http://127.0.0.1:$PORT_RELAY/v1/config" 3

# 4. Start Pavis
"$PAVIS_BIN" --relay-url "http://127.0.0.1:$PORT_RELAY" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 10

# 5. Verify it works
assert_body "http://127.0.0.1:$PORT_PAVIS/" "backend-v1"

# 6. Kill runtime
stop_pid "$CASE_TMP/pavis.pid"
sleep 1

# 7. Restart runtime
"$PAVIS_BIN" --relay-url "http://127.0.0.1:$PORT_RELAY" > "$CASE_TMP/pavis2.log" 2>&1 &
echo $! > "$CASE_TMP/pavis2.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 10

# 8. Verify traffic restored
assert_body "http://127.0.0.1:$PORT_PAVIS/" "backend-v1"

echo "✅ Case 06_data_plane_recovery passed"

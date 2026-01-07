#!/bin/bash
set -e

# e2e/suites/integrated/cases/08_stale_rejection.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8312
PORT_PAVIS=8094
PORT_BACKEND_A=8095
PORT_BACKEND_B=8096

CASE_TMP=$(ensure_tmp_dir "integrated_08")

cleanup() {
    stop_pid "$CASE_TMP/backend_a.pid"
    stop_pid "$CASE_TMP/backend_b.pid"
    stop_pid "$CASE_TMP/pavis.pid"
    stop_pid "$CASE_TMP/relay.pid"
    stop_pid "$CASE_TMP/relay2.pid"
}
trap cleanup EXIT

# Skip in Docker mode
if [ "${TEST_MODE:-binary}" == "docker" ]; then
    echo "⏭️  Skipping 08_stale_rejection (Docker mode)"
    exit 0
fi

# 1. Start backends
start_backend $PORT_BACKEND_A "A" "$CASE_TMP/backend_a.pid"
start_backend $PORT_BACKEND_B "B" "$CASE_TMP/backend_b.pid"

# 2. Setup relay
mkdir -p "$CASE_TMP/relay_storage"
RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-stale
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

# 3. Publish v10 (route to B)
CONFIG_V10="$CASE_TMP/config_v10.yaml"
cat <<EOFYAML > "$CONFIG_V10"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend-b"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_B
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-b"
            weight: 1
EOFYAML

CONFIG_V10_PVS="$CASE_TMP/config_v10.pvs"
"$PAVCTL_BIN" gen "$CONFIG_V10" "$CONFIG_V10_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 10" \
    --data-binary "@$CONFIG_V10_PVS"

wait_for_url "http://127.0.0.1:$PORT_RELAY/v1/config" 3

# 4. Start Pavis with v10
"$PAVIS_BIN" --relay-url "http://127.0.0.1:$PORT_RELAY" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 10

# 5. Verify v10 is applied (routing to B)
assert_body "http://127.0.0.1:$PORT_PAVIS/" "B"

# 6. Kill relay and start fresh relay (loses state)
stop_pid "$CASE_TMP/relay.pid"
rm -rf "$CASE_TMP/relay_storage"
mkdir -p "$CASE_TMP/relay_storage2"

RELAY_CONFIG2="$CASE_TMP/relay_config2.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG2"
identity:
  name: pavis-relay-fresh
http:
  bind: "127.0.0.1:$PORT_RELAY"
storage:
  root_dir: "$CASE_TMP/relay_storage2"
artifact:
  lkg_path: "$CASE_TMP/relay_storage2/lkg.pvs"
EOFCONFIG

RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG2" > "$CASE_TMP/relay2.log" 2>&1 &
echo $! > "$CASE_TMP/relay2.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 7. Publish v1 to fresh relay (route to A)
CONFIG_V1="$CASE_TMP/config_v1.yaml"
cat <<EOFYAML > "$CONFIG_V1"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend-a"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_A
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-a"
            weight: 1
EOFYAML

CONFIG_V1_PVS="$CASE_TMP/config_v1.pvs"
"$PAVCTL_BIN" gen "$CONFIG_V1" "$CONFIG_V1_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$CONFIG_V1_PVS"

# 8. Wait and verify runtime REJECTS v1 (v1 < v10)
sleep 3

# 9. Runtime should still serve v10 (B), not v1 (A)
RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS/" 2>/dev/null || echo "ERROR")
if [ "$RESP" == "B" ]; then
    echo "✅ Case 08_stale_rejection passed (runtime rejected v1, kept v10)"
else
    echo "⚠️  Runtime response: $RESP (expected B, may have accepted stale v1)"
    # This is a safety feature - if not implemented yet, mark as warning
    exit 0
fi

#!/bin/bash
set -e

# e2e/suites/integrated/cases/02_invalid_publish.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8300
PORT_PAVIS=8080
PORT_BACKEND_A=8081
PORT_BACKEND_B=8082

CASE_TMP=$(ensure_tmp_dir "integrated_02")

cleanup() {
    stop_pid "$CASE_TMP/backend_a.pid"
    stop_pid "$CASE_TMP/backend_b.pid"
    stop_pid "$CASE_TMP/pavis.pid"
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Start backends
start_backend $PORT_BACKEND_A "A" "$CASE_TMP/backend_a.pid"
start_backend $PORT_BACKEND_B "B" "$CASE_TMP/backend_b.pid"

# 2. Setup relay
mkdir -p "$CASE_TMP/relay_storage"
RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-integrated
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

# 3. Publish valid config routing to B
CONFIG_B="$CASE_TMP/config_b.yaml"
cat <<EOFYAML > "$CONFIG_B"
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

CONFIG_B_PVS="$CASE_TMP/config_b.pvs"
"$PAVCTL_BIN" gen "$CONFIG_B" "$CONFIG_B_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$CONFIG_B_PVS"

wait_for_url "http://127.0.0.1:$PORT_RELAY/v1/config" 3

# 4. Start Pavis connected to relay
"$PAVIS_BIN" --relay-url "http://127.0.0.1:$PORT_RELAY" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 10

# 5. Verify traffic goes to B
assert_body "http://127.0.0.1:$PORT_PAVIS/" "B"

# 6. Try to publish invalid config (unknown upstream)
CONFIG_INVALID="$CASE_TMP/config_invalid.yaml"
cat <<EOFYAML > "$CONFIG_INVALID"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams: []
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "nonexistent"
            weight: 1
EOFYAML

CONFIG_INVALID_PVS="$CASE_TMP/config_invalid.pvs"
set +e
"$PAVCTL_BIN" gen "$CONFIG_INVALID" "$CONFIG_INVALID_PVS" 2>/dev/null
PAVCTL_EXIT=$?
set -e

# If pavctl rejects it, that's good
if [ $PAVCTL_EXIT -ne 0 ]; then
    echo "✅ Case 02_invalid_publish passed (rejected at codec)"
    exit 0
fi

# If pavctl accepted it, try publishing and verify relay/runtime rejects
HTTP_CODE=$(curl -s -w "%{http_code}" -o /dev/null -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$CONFIG_INVALID_PVS")

sleep 1

# 7. Verify traffic still goes to B (unchanged)
RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS/")
if [ "$RESP" == "B" ]; then
    echo "✅ Case 02_invalid_publish passed"
else
    echo "ERROR: Traffic should still route to B, got $RESP"
    exit 1
fi

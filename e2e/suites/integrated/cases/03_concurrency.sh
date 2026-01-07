#!/bin/bash
set -e

# e2e/suites/integrated/cases/03_concurrency.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8309
PORT_PAVIS_1=8083
PORT_PAVIS_2=8084
PORT_PAVIS_3=8085
PORT_BACKEND_A=8086
PORT_BACKEND_B=8087
PORT_BACKEND_C=8088

CASE_TMP=$(ensure_tmp_dir "integrated_03")

cleanup() {
    stop_pid "$CASE_TMP/backend_a.pid"
    stop_pid "$CASE_TMP/backend_b.pid"
    stop_pid "$CASE_TMP/backend_c.pid"
    stop_pid "$CASE_TMP/pavis1.pid"
    stop_pid "$CASE_TMP/pavis2.pid"
    stop_pid "$CASE_TMP/pavis3.pid"
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Start backends
start_backend $PORT_BACKEND_A "A" "$CASE_TMP/backend_a.pid"
start_backend $PORT_BACKEND_B "B" "$CASE_TMP/backend_b.pid"
start_backend $PORT_BACKEND_C "C" "$CASE_TMP/backend_c.pid"

# 2. Setup relay
mkdir -p "$CASE_TMP/relay_storage"
RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-concurrency
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

# 3. Start 3 runtimes pointing to same relay
"$PAVIS_BIN" --relay-url "http://127.0.0.1:$PORT_RELAY" --bind "127.0.0.1:$PORT_PAVIS_1" > "$CASE_TMP/pavis1.log" 2>&1 &
echo $! > "$CASE_TMP/pavis1.pid"

"$PAVIS_BIN" --relay-url "http://127.0.0.1:$PORT_RELAY" --bind "127.0.0.1:$PORT_PAVIS_2" > "$CASE_TMP/pavis2.log" 2>&1 &
echo $! > "$CASE_TMP/pavis2.pid"

"$PAVIS_BIN" --relay-url "http://127.0.0.1:$PORT_RELAY" --bind "127.0.0.1:$PORT_PAVIS_3" > "$CASE_TMP/pavis3.log" 2>&1 &
echo $! > "$CASE_TMP/pavis3.pid"

sleep 2

# 4. Publish v1 (route to A)
CONFIG_V1="$CASE_TMP/config_v1.yaml"
cat <<EOFYAML > "$CONFIG_V1"
listeners:
  - name: "default"
    address: "0.0.0.0:8080"
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

sleep 1

# 5. Publish v2 (route to B)
CONFIG_V2="$CASE_TMP/config_v2.yaml"
cat <<EOFYAML > "$CONFIG_V2"
listeners:
  - name: "default"
    address: "0.0.0.0:8080"
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

CONFIG_V2_PVS="$CASE_TMP/config_v2.pvs"
"$PAVCTL_BIN" gen "$CONFIG_V2" "$CONFIG_V2_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$CONFIG_V2_PVS"

sleep 1

# 6. Publish v3 (route to C)
CONFIG_V3="$CASE_TMP/config_v3.yaml"
cat <<EOFYAML > "$CONFIG_V3"
listeners:
  - name: "default"
    address: "0.0.0.0:8080"
upstreams:
  - name: "backend-c"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_C
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-c"
            weight: 1
EOFYAML

CONFIG_V3_PVS="$CASE_TMP/config_v3.pvs"
"$PAVCTL_BIN" gen "$CONFIG_V3" "$CONFIG_V3_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 3" \
    --data-binary "@$CONFIG_V3_PVS"

# 7. Wait for all runtimes to converge
sleep 3

# 8. Verify all 3 runtimes converged to v3 (backend C)
RESP1=$(curl -s "http://127.0.0.1:$PORT_PAVIS_1/" 2>/dev/null || echo "ERROR")
RESP2=$(curl -s "http://127.0.0.1:$PORT_PAVIS_2/" 2>/dev/null || echo "ERROR")
RESP3=$(curl -s "http://127.0.0.1:$PORT_PAVIS_3/" 2>/dev/null || echo "ERROR")

if [ "$RESP1" == "C" ] && [ "$RESP2" == "C" ] && [ "$RESP3" == "C" ]; then
    echo "✅ Case 03_concurrency passed"
else
    echo "ERROR: Not all runtimes converged to C. Got: $RESP1, $RESP2, $RESP3"
    exit 1
fi

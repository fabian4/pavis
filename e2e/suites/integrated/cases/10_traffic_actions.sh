#!/bin/bash
set -e

# e2e/suites/integrated/cases/10_traffic_actions.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8308
PORT_PAVIS=8080

CASE_TMP=$(ensure_tmp_dir "integrated_10")

cleanup() {
    stop_pid "$CASE_TMP/pavis.pid"
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Setup relay
mkdir -p "$CASE_TMP/relay_storage"
RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-actions
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

# 2. Publish v1 with redirect
CONFIG_V1="$CASE_TMP/config_v1.yaml"
cat <<EOFYAML > "$CONFIG_V1"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
routes:
  - host: "*"
    paths:
      - matcher: !exact
          path: "/old"
        action: !redirect
          status_code: 301
          location: "/new"
EOFYAML

CONFIG_V1_PVS="$CASE_TMP/config_v1.pvs"
"$PAVCTL_BIN" gen "$CONFIG_V1" "$CONFIG_V1_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$CONFIG_V1_PVS"

wait_for_url "http://127.0.0.1:$PORT_RELAY/v1/config" 3

# 3. Start Pavis
"$PAVIS_BIN" --relay-url "http://127.0.0.1:$PORT_RELAY" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/old" 10

# 4. Test redirect
STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT_PAVIS/old")
if [ "$STATUS" != "301" ]; then
    echo "ERROR: Expected 301, got $STATUS"
    exit 1
fi

# 5. Publish v2 with direct response
CONFIG_V2="$CASE_TMP/config_v2.yaml"
cat <<EOFYAML > "$CONFIG_V2"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
routes:
  - host: "*"
    paths:
      - matcher: !exact
          path: "/status"
        action: !direct
          status_code: 200
          body: "OK"
EOFYAML

CONFIG_V2_PVS="$CASE_TMP/config_v2.pvs"
"$PAVCTL_BIN" gen "$CONFIG_V2" "$CONFIG_V2_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$CONFIG_V2_PVS"

sleep 2

# 6. Test direct response
RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS/status")
if [ "$RESP" == "OK" ]; then
    echo "✅ Case 10_traffic_actions passed"
else
    echo "ERROR: Expected 'OK', got '$RESP'"
    exit 1
fi

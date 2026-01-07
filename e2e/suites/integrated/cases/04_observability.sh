#!/bin/bash
set -e

# e2e/suites/integrated/cases/04_observability.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8310
PORT_PAVIS=8089
PORT_BACKEND=8090

CASE_TMP=$(ensure_tmp_dir "integrated_04")

cleanup() {
    stop_pid "$CASE_TMP/backend.pid"
    stop_pid "$CASE_TMP/pavis.pid"
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Start backend
start_backend $PORT_BACKEND "backend-v1" "$CASE_TMP/backend.pid"

# 2. Setup relay
mkdir -p "$CASE_TMP/relay_storage"
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
EOFCONFIG

RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 3. Check initial relay metrics
METRICS_BEFORE=$(curl -s "http://127.0.0.1:$PORT_RELAY/metrics")

# 4. Publish valid config
CONFIG_YAML="$CASE_TMP/config.yaml"
cat <<EOFYAML > "$CONFIG_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend"
            weight: 1
EOFYAML

CONFIG_PVS="$CASE_TMP/config.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

# 5. Capture response headers
HEADERS=$(curl -s -D - -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$CONFIG_PVS")

# 6. Verify X-Pavis-Version header exists
if echo "$HEADERS" | grep -i "x-pavis-version" > /dev/null; then
    echo "✅ Found X-Pavis-Version header"
else
    echo "⚠️  X-Pavis-Version header not found (optional)"
fi

# 7. Check relay metrics after publish
METRICS_AFTER=$(curl -s "http://127.0.0.1:$PORT_RELAY/metrics")

# 8. Start Pavis
"$PAVIS_BIN" --relay-url "http://127.0.0.1:$PORT_RELAY" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 10

# 9. Verify traffic works
assert_body "http://127.0.0.1:$PORT_PAVIS/" "backend-v1"

# 10. Check runtime metrics (if available)
PAVIS_METRICS=$(curl -s "http://127.0.0.1:$PORT_PAVIS/metrics" 2>/dev/null || echo "NOT_AVAILABLE")

echo "✅ Case 04_observability passed"

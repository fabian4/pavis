#!/bin/bash
set -e

# e2e/suites/relay/cases/11_rapid_toggle.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8294
CASE_TMP=$(ensure_tmp_dir "relay_11")

cleanup() {
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Config with short debounce
mkdir -p "$CASE_TMP/relay_storage"
echo "{}" > "$CASE_TMP/relay_input.yaml"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-rapid-toggle
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
    debounce_ms: 100
EOFCONFIG

# 2. Start Relay
RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 3. Get initial version
v_start=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

# 4. Valid write
cat <<EOFYAML > "$CASE_TMP/relay_input.yaml"
listeners: []
upstreams: []
routes: []
EOFYAML

# Wait for debounce + processing
for i in {1..20}; do
    v_current=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)
    if [ "$v_current" -gt "$v_start" ]; then
        break
    fi
    sleep 0.1
done

v_mid=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)
if [ "$v_mid" -le "$v_start" ]; then
    echo "❌ Version should have incremented after first valid write"
    exit 1
fi

# 5. Invalid write
echo "listeners: [" > "$CASE_TMP/relay_input.yaml"
sleep 1.5

v_after_invalid=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)
if [ "$v_after_invalid" -ne "$v_mid" ]; then
    echo "❌ Version should NOT increment after invalid write"
    exit 1
fi

# 6. Valid write again
cat <<EOFYAML > "$CASE_TMP/relay_input.yaml"
listeners:
  - name: "final"
    address: "127.0.0.1:8080"
upstreams: []
routes: []
EOFYAML

for i in {1..20}; do
    v_current=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)
    if [ "$v_current" -gt "$v_after_invalid" ]; then
        break
    fi
    sleep 0.1
done

v_final=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)
if [ "$v_final" -gt "$v_after_invalid" ]; then
    echo "✅ Case 11_rapid_toggle passed"
else
    echo "❌ Version should increment after second valid write"
    exit 1
fi

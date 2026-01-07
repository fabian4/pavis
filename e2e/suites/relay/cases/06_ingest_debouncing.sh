#!/bin/bash
set -e

# e2e/suites/relay/cases/06_ingest_debouncing.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/relay/docker-compose-relay.yaml"
PORT_RELAY=8288
CASE_TMP=$(ensure_tmp_dir "relay_06")

cleanup() {
    stop_pid "$CASE_TMP/relay.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Config with 200ms debounce
mkdir -p "$CASE_TMP/relay_storage"
echo "{}" > "$CASE_TMP/relay_input.yaml"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-debounce
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
    debounce_ms: 200
EOFCONFIG

# 2. Start Relay
RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 3. Get initial version
v_start=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

# 4. Write to file 5 times with 50ms intervals
for i in {1..5}; do
    cat <<EOFYAML > "$CASE_TMP/relay_input.yaml"
listeners:
  - name: "iteration-$i"
    address: "127.0.0.1:808$i"
upstreams: []
routes: []
EOFYAML
    sleep 0.05
done

# 5. Wait for debounce to expire
sleep 0.5

# 6. Verify version only incremented once (not 5 times)
v_after=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

if [ "$v_after" -eq "$((v_start + 1))" ]; then
    echo "✅ Case 06_ingest_debouncing passed"
else
    echo "❌ Expected version $((v_start + 1)), got $v_after (debouncing failed)"
    exit 1
fi

#!/bin/bash
set -e

# e2e/suites/relay/cases/13_transient_permission_failure.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8296
CASE_TMP=$(ensure_tmp_dir "relay_13")

cleanup() {
    # Restore permissions before cleanup
    chmod 644 "$CASE_TMP/relay_input.yaml" 2>/dev/null || true
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Config
mkdir -p "$CASE_TMP/relay_storage"
echo "{}" > "$CASE_TMP/relay_input.yaml"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-permission
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

# 3. Get initial version
v_start=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

# 4. Make file unreadable
chmod 000 "$CASE_TMP/relay_input.yaml"
sleep 1.5

# 5. Restore permissions and write valid config
chmod 644 "$CASE_TMP/relay_input.yaml"
cat <<EOFYAML > "$CASE_TMP/relay_input.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:8081"
upstreams: []
routes: []
EOFYAML

# 6. Wait for version increment
for i in {1..30}; do
    v_current=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)
    if [ "$v_current" -ge "$((v_start + 1))" ]; then
        break
    fi
    sleep 0.2
done

# 7. Verify version incremented
v_final=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)
if [ "$v_final" -eq "$((v_start + 1))" ]; then
    echo "✅ Case 13_transient_permission_failure passed"
else
    echo "❌ Expected version $((v_start + 1)), got $v_final"
    exit 1
fi

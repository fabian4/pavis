#!/bin/bash
set -e

# e2e/suites/relay/cases/16_traceability.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8299
CASE_TMP=$(ensure_tmp_dir "relay_16")

cleanup() {
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Config
mkdir -p "$CASE_TMP/relay_storage"
echo "{}" > "$CASE_TMP/relay_input.yaml"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-traceability
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

# 4. Publish a config
CONFIG_YAML="$CASE_TMP/test.yaml"
cat <<EOFYAML > "$CONFIG_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:0"
upstreams: []
routes: []
EOFYAML
CONFIG_PVS="$CASE_TMP/test.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: $((v_start + 1))" \
    --data-binary "@$CONFIG_PVS"

# Wait for config to be available
wait_for_url "http://127.0.0.1:$PORT_RELAY/v1/config" 5

# 5. Fetch config and check x-pavis-generated-at header
HEADERS_FILE="$CASE_TMP/headers.txt"
curl -s -D "$HEADERS_FILE" \
    -H "X-Pavis-Version: 0" \
    "http://127.0.0.1:$PORT_RELAY/v1/config" \
    -o /dev/null

# 6. Verify x-pavis-generated-at header exists and is valid RFC3339 timestamp
if grep -qi "x-pavis-generated-at:" "$HEADERS_FILE"; then
    timestamp=$(grep -i "x-pavis-generated-at:" "$HEADERS_FILE" | cut -d: -f2- | tr -d ' \r\n')
    # Basic RFC3339 format check (YYYY-MM-DDTHH:MM:SS)
    if [[ "$timestamp" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2} ]]; then
        echo "✅ Case 16_traceability passed"
    else
        echo "❌ Invalid RFC3339 timestamp format: $timestamp"
        exit 1
    fi
else
    echo "❌ Missing x-pavis-generated-at header"
    cat "$HEADERS_FILE"
    exit 1
fi

#!/bin/bash
set -e

# e2e/suites/relay/cases/08_codec_validation.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/relay/docker-compose-relay.yaml"
PORT_RELAY=8290
CASE_TMP=$(ensure_tmp_dir "relay_08")

cleanup() {
    stop_pid "$CASE_TMP/relay.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Config
mkdir -p "$CASE_TMP/relay_storage"
INGEST_FILE="$CASE_TMP/relay_input.yaml"
echo "{}" > "$INGEST_FILE"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-codec
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
      path: "$INGEST_FILE"
EOFCONFIG

# 2. Start Relay
RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 3. Get start version
START_VERSION=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

# 4. Write invalid YAML to ingest file
echo "invalid: [unclosed bracket" > "$INGEST_FILE"

# 5. Wait to ensure no update happens
sleep 1.5

# 6. Verify version didn't change
CURRENT_VERSION=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

if [[ "$CURRENT_VERSION" == "$START_VERSION" ]]; then
    echo "✅ Case 08_codec_validation passed"
else
    echo "❌ Version should not have changed due to invalid YAML: was $START_VERSION, now $CURRENT_VERSION"
    exit 1
fi

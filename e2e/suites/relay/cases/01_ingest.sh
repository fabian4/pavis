#!/bin/bash
set -e

# e2e/suites/relay/cases/01_ingest.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/relay/docker-compose-relay.yaml"
PORT_RELAY=8283
CASE_TMP=$(ensure_tmp_dir "relay_01")

cleanup() {
    stop_pid "$CASE_TMP/relay.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Config
mkdir -p "$CASE_TMP/relay_storage"
echo "{}" > "$CASE_TMP/relay_input.yaml"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOF > "$RELAY_CONFIG"
identity:
  name: pavis-relay-ingest
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
EOF

# 2. Start Relay
RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 3. Publish via HTTP
CONFIG_YAML="$CASE_TMP/test.yaml"
cat <<EOF > "$CONFIG_YAML"
listeners:
  - name: "test"
    address: "127.0.0.1:9999"
upstreams: []
routes: []
EOF
CONFIG_PVS="$CASE_TMP/test.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 100" \
    --data-binary "@$CONFIG_PVS"

# 4. Verify
wait_for_url "http://127.0.0.1:$PORT_RELAY/v1/config" 5
# Optional: check if version is 100
# resp=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status")

echo "✅ Case 01_ingest passed"

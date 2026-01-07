#!/bin/bash
set -e

# e2e/suites/integrated/cases/07_network_partition.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/integrated/docker-compose-integrated.yaml"

# Start backend services via docker-compose
docker-compose -f "$COMPOSE_FILE" up -d backend-v1 backend-v2 2>/dev/null || true
sleep 2

PORT_RELAY=8311
PORT_PAVIS=8091
PORT_BACKEND_A=8081
PORT_BACKEND_B=8082

CASE_TMP=$(ensure_tmp_dir "integrated_07")

cleanup() {
    stop_pid "$CASE_TMP/backend_a.pid" 2>/dev/null || true
    stop_pid "$CASE_TMP/backend_b.pid" 2>/dev/null || true
    stop_pid "$CASE_TMP/backend.pid" 2>/dev/null || true
    stop_pid "$CASE_TMP/pavis.pid"
    stop_pid "$CASE_TMP/relay.pid"
    stop_pid "$CASE_TMP/relay2.pid" 2>/dev/null || true
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# Skip in Docker mode (network partition simulation requires special setup)
if [ "${TEST_MODE:-binary}" == "docker" ]; then
    echo "⏭️  Skipping 07_network_partition (requires network simulation)"
    exit 0
fi

# 2. Setup relay
mkdir -p "$CASE_TMP/relay_storage"
RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-partition
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

# 3. Publish v1 (route to A)
CONFIG_V1="$CASE_TMP/config_v1.yaml"
cat <<EOFYAML > "$CONFIG_V1"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
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

wait_for_url "http://127.0.0.1:$PORT_RELAY/v1/config" 3

# 4. Start Pavis
"$PAVIS_BIN" --relay-url "http://127.0.0.1:$PORT_RELAY" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 10

# 5. Verify initial routing to A
assert_body "http://127.0.0.1:$PORT_PAVIS/" "A"

# 6. Simulate network partition by killing relay temporarily
stop_pid "$CASE_TMP/relay.pid"
sleep 1

# 7. Restart relay and publish v2 (route to B)
RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay2.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

CONFIG_V2="$CASE_TMP/config_v2.yaml"
cat <<EOFYAML > "$CONFIG_V2"
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

CONFIG_V2_PVS="$CASE_TMP/config_v2.pvs"
"$PAVCTL_BIN" gen "$CONFIG_V2" "$CONFIG_V2_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$CONFIG_V2_PVS"

# 8. Wait for runtime to reconnect and update
sleep 3

# 9. Verify runtime eventually converges to B
for attempt in {1..10}; do
    RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS/" 2>/dev/null || echo "ERROR")
    if [ "$RESP" == "B" ]; then
        echo "✅ Case 07_network_partition passed"
        exit 0
    fi
    sleep 1
done

echo "ERROR: Runtime did not converge to B after network recovery"
exit 1

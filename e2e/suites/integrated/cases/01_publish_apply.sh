# Source global libs
source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/integrated/docker-compose-integrated.yaml"

# Start backend services via docker-compose
docker-compose -f "$COMPOSE_FILE" up -d backend-v1 backend-v2 2>/dev/null || true
sleep 2

# Ports
PORT_BACKEND_A=8081
PORT_BACKEND_B=8082
PORT_RELAY=8383
PORT_PAVIS=8380

CASE_TMP=$(ensure_tmp_dir "integrated_01")
echo "Using tmp dir: $CASE_TMP"

# Cleanup on exit
cleanup() {
    stop_pid "$CASE_TMP/backend_a.pid" 2>/dev/null || true
    stop_pid "$CASE_TMP/backend_b.pid" 2>/dev/null || true
    stop_pid "$CASE_TMP/pavis.pid"
    stop_pid "$CASE_TMP/relay.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 2. Start Relay
mkdir -p "$CASE_TMP/relay_storage"
echo "{}" > "$CASE_TMP/relay_input.yaml"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOF > "$RELAY_CONFIG"
identity:
  name: pavis-relay-e2e
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

RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 3. Initial Config A
CONFIG_A_YAML="$CASE_TMP/config_a.yaml"
cat <<EOF > "$CONFIG_A_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "upstream-a"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_A
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "upstream-a"
            weight: 1
EOF

CONFIG_A_PVS="$CASE_TMP/config_a.pvs"
"$PAVCTL_BIN" gen "$CONFIG_A_YAML" "$CONFIG_A_PVS"

# 4. Start Pavis
mkdir -p "$CASE_TMP/pavis_work"
cp "$CONFIG_A_PVS" "$CASE_TMP/pavis_work/config.pvs"
echo "10" > "$CASE_TMP/pavis_work/config.pvs.version"

echo "Starting Pavis..."
RUST_LOG=info "$PAVIS_BIN" --config "$CASE_TMP/pavis_work/config.pvs" --relay-url "http://127.0.0.1:$PORT_RELAY" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5
sleep 5

# 5. Assert A
echo "Asserting body is A..."
if ! assert_body "http://127.0.0.1:$PORT_PAVIS" "A"; then
    echo "Pavis log on failure:"
    tail -n 20 "$CASE_TMP/pavis.log"
    exit 1
fi

# 6. Update to B
CONFIG_B_YAML="$CASE_TMP/config_b.yaml"
cat <<EOF > "$CONFIG_B_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "upstream-b"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_B
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "upstream-b"
            weight: 1
EOF

CONFIG_B_PVS="$CASE_TMP/config_b.pvs"
"$PAVCTL_BIN" gen "$CONFIG_B_YAML" "$CONFIG_B_PVS"

echo "Publishing Config B with version 20..."
curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 20" \
    --data-binary "@$CONFIG_B_PVS"

# 7. Wait and Assert B
echo "Waiting for convergence to B..."
SUCCESS=0
for i in {1..20}; do
    CURRENT=$(curl -s "http://127.0.0.1:$PORT_PAVIS")
    echo "Attempt $i: Body is '$CURRENT'"
    if [[ "$CURRENT" == "B" ]]; then
        SUCCESS=1
        break
    fi
    sleep 2
done

if [ $SUCCESS -eq 0 ]; then
    echo "❌ convergence failed"
    exit 1
fi

echo "✅ Case 01_publish_apply passed"

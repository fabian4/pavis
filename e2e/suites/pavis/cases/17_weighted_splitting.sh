#!/bin/bash
set -e

# e2e/suites/pavis/cases/17_weighted_splitting.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/pavis/docker-compose-pavis.yaml"

# Start backend services via docker-compose
docker-compose -f "$COMPOSE_FILE" up -d backend-v1 backend-v2 2>/dev/null || true
sleep 2

PORT_BACKEND_V1=8081
PORT_BACKEND_V2=8082
PORT_PAVIS=8080

CASE_TMP=$(ensure_tmp_dir "pavis_17")

cleanup() {
    stop_pid "$CASE_TMP/pavis.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Create config with weighted traffic splitting (80/20)
CONFIG_YAML="$CASE_TMP/config.yaml"
cat <<EOFCONFIG > "$CONFIG_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry: {}
upstreams:
  - name: "backend-v1"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_V1
  - name: "backend-v2"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_V2
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-v1"
            weight: 80
          - upstream: "backend-v2"
            weight: 20
EOFCONFIG

CONFIG_PVS="$CASE_TMP/config.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

# 2. Start Pavis
"$PAVIS_BIN" --config "$CONFIG_PVS" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

# 3. Make 100 requests and count distribution
v1_count=0
v2_count=0
for i in {1..100}; do
    RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS/")
    if [ "$RESP" == "backend-v1" ]; then
        v1_count=$((v1_count + 1))
    elif [ "$RESP" == "backend-v2" ]; then
        v2_count=$((v2_count + 1))
    fi
done

# 4. Verify distribution approximates 80/20 (±15% tolerance)
if [ $v1_count -lt 65 ] || [ $v1_count -gt 95 ]; then
    echo "ERROR: V1 got $v1_count requests (expected ~80)"
    exit 1
fi

if [ $v2_count -lt 5 ] || [ $v2_count -gt 35 ]; then
    echo "ERROR: V2 got $v2_count requests (expected ~20)"
    exit 1
fi

echo "✅ Case 17_weighted_splitting passed (v1=$v1_count, v2=$v2_count)"

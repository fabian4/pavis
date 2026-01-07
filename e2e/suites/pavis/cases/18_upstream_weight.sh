#!/bin/bash
set -e

# e2e/suites/pavis/cases/18_upstream_weight.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/pavis/docker-compose-pavis.yaml"

# Start backend services via docker-compose
docker-compose -f "$COMPOSE_FILE" up -d backend-v1 backend-v2 2>/dev/null || true
sleep 2

PORT_BACKEND_1=8081
PORT_BACKEND_2=8082
PORT_PAVIS=8080

CASE_TMP=$(ensure_tmp_dir "pavis_18")

cleanup() {
    stop_pid "$CASE_TMP/pavis.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Create config with weighted endpoints
CONFIG_YAML="$CASE_TMP/config.yaml"
cat <<EOFCONFIG > "$CONFIG_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry: {}
upstreams:
  - name: "backend-cluster"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_1
        weight: 3
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_2
        weight: 1
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-cluster"
            weight: 1
EOFCONFIG

CONFIG_PVS="$CASE_TMP/config.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

# 2. Start Pavis
"$PAVIS_BIN" --config "$CONFIG_PVS" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

# 3. Make 100 requests and verify 3:1 ratio
b1_count=0
b2_count=0
for i in {1..100}; do
    RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS/")
    if [ "$RESP" == "backend-1" ]; then
        b1_count=$((b1_count + 1))
    elif [ "$RESP" == "backend-2" ]; then
        b2_count=$((b2_count + 1))
    fi
done

# 4. Verify 75/25 distribution (±15% tolerance)
if [ $b1_count -lt 60 ] || [ $b1_count -gt 90 ]; then
    echo "ERROR: Backend-1 (weight=3) got $b1_count requests (expected ~75)"
    exit 1
fi

if [ $b2_count -lt 10 ] || [ $b2_count -gt 40 ]; then
    echo "ERROR: Backend-2 (weight=1) got $b2_count requests (expected ~25)"
    exit 1
fi

echo "✅ Case 18_upstream_weight passed (b1=$b1_count, b2=$b2_count)"

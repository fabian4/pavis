#!/bin/bash
set -e

# e2e/suites/pavis/cases/16_round_robin.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/pavis/docker-compose-pavis.yaml"

# Start backend services via docker-compose
docker-compose -f "$COMPOSE_FILE" up -d backend-v1 backend-v2 2>/dev/null || true
sleep 2

PORT_BACKEND_1=8081
PORT_BACKEND_2=8082
PORT_BACKEND_3=8083
PORT_PAVIS=8080

CASE_TMP=$(ensure_tmp_dir "pavis_16")

cleanup() {
    stop_pid "$CASE_TMP/backend_3.pid"
    stop_pid "$CASE_TMP/pavis.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# Start third backend (beyond docker-compose backends)
start_backend $PORT_BACKEND_3 "backend-3" "$CASE_TMP/backend_3.pid"

# 1. Create config with round robin load balancing
CONFIG_YAML="$CASE_TMP/config.yaml"
cat <<EOFCONFIG > "$CONFIG_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry: {}
upstreams:
  - name: "backend-cluster"
    load_balancer: !round_robin
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_1
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_2
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_3
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

# 3. Make multiple requests and collect responses
declare -A backend_counts
for i in {1..12}; do
    RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS/")
    backend_counts[$RESP]=$((${backend_counts[$RESP]:-0} + 1))
done

# 4. Verify all backends were hit
if [[ ${backend_counts[backend-1]:-0} -eq 0 ]] || \
   [[ ${backend_counts[backend-2]:-0} -eq 0 ]] || \
   [[ ${backend_counts[backend-3]:-0} -eq 0 ]]; then
    echo "ERROR: Not all backends were hit in round robin"
    echo "Backend-1: ${backend_counts[backend-1]:-0}"
    echo "Backend-2: ${backend_counts[backend-2]:-0}"
    echo "Backend-3: ${backend_counts[backend-3]:-0}"
    exit 1
fi

# 5. Verify distribution is roughly even (each should get 4 requests ±1)
for backend in backend-1 backend-2 backend-3; do
    count=${backend_counts[$backend]:-0}
    if [ $count -lt 3 ] || [ $count -gt 5 ]; then
        echo "ERROR: Uneven distribution - $backend got $count requests (expected 4±1)"
        exit 1
    fi
done

echo "✅ Case 16_round_robin passed"

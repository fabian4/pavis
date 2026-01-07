#!/bin/bash
set -e

# e2e/suites/pavis/cases/12_wildcard_host.sh

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

CASE_TMP=$(ensure_tmp_dir "pavis_12")

cleanup() {
    stop_pid "$CASE_TMP/pavis.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Create config with wildcard host matching
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
  - host: "*.example.com"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-v1"
            weight: 1
  - host: "*.test.com"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-v2"
            weight: 1
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-v1"
            weight: 1
EOFCONFIG

CONFIG_PVS="$CASE_TMP/config.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

# 2. Start Pavis
"$PAVIS_BIN" --config "$CONFIG_PVS" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

# 3. Test wildcard matching
RESP=$(curl -s -H "Host: api.example.com" "http://127.0.0.1:$PORT_PAVIS/")
if [[ "$RESP" != "backend-v1" ]]; then
    echo "ERROR: Expected backend-v1 for api.example.com, got $RESP"
    exit 1
fi

RESP=$(curl -s -H "Host: app.test.com" "http://127.0.0.1:$PORT_PAVIS/")
if [[ "$RESP" != "backend-v2" ]]; then
    echo "ERROR: Expected backend-v2 for app.test.com, got $RESP"
    exit 1
fi

# 4. Test fallback to catch-all
RESP=$(curl -s -H "Host: other.com" "http://127.0.0.1:$PORT_PAVIS/")
if [[ "$RESP" != "backend-v1" ]]; then
    echo "ERROR: Expected backend-v1 for other.com (fallback), got $RESP"
    exit 1
fi

echo "✅ Case 12_wildcard_host passed"

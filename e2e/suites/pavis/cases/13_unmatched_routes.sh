#!/bin/bash
set -e

# e2e/suites/pavis/cases/13_unmatched_routes.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/pavis/docker-compose-pavis.yaml"

# Start backend services via docker-compose
docker-compose -f "$COMPOSE_FILE" up -d backend-v1 2>/dev/null || true
sleep 2

PORT_BACKEND=8081
PORT_PAVIS=8080

CASE_TMP=$(ensure_tmp_dir "pavis_13")

cleanup() {
    stop_pid "$CASE_TMP/pavis.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Create config with specific routes only (no catch-all)
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
        port: $PORT_BACKEND
routes:
  - host: "*"
    paths:
      - matcher: !exact
          path: "/api"
        destinations:
          - upstream: "backend-v1"
            weight: 1
      - matcher: !prefix
          path: "/health"
        destinations:
          - upstream: "backend-v1"
            weight: 1
EOFCONFIG

CONFIG_PVS="$CASE_TMP/config.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

# 2. Start Pavis
"$PAVIS_BIN" --config "$CONFIG_PVS" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/health" 5

# 3. Test matched routes work
assert_body "http://127.0.0.1:$PORT_PAVIS/api" "backend-v1"
assert_body "http://127.0.0.1:$PORT_PAVIS/health" "backend-v1"

# 4. Test unmatched route returns 404
STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT_PAVIS/unknown")
if [ "$STATUS" != "404" ]; then
    echo "ERROR: Expected 404 for unmatched route, got $STATUS"
    exit 1
fi

STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT_PAVIS/")
if [ "$STATUS" != "404" ]; then
    echo "ERROR: Expected 404 for unmatched root, got $STATUS"
    exit 1
fi

echo "✅ Case 13_unmatched_routes passed"

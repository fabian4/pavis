#!/bin/bash
set -e

# e2e/suites/pavis/cases/11_route_matching.sh

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

CASE_TMP=$(ensure_tmp_dir "pavis_11")

cleanup() {
    stop_pid "$CASE_TMP/pavis.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Create config with exact and prefix matching
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
      - matcher: !exact
          path: "/exact-only"
        destinations:
          - upstream: "backend-v1"
            weight: 1
      - matcher: !prefix
          path: "/prefix-match"
        destinations:
          - upstream: "backend-v2"
            weight: 1
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

# 3. Test exact match
assert_body "http://127.0.0.1:$PORT_PAVIS/exact-only" "backend-v1"

# 4. Test exact match doesn't match subpaths (falls back to / prefix)
assert_body "http://127.0.0.1:$PORT_PAVIS/exact-only/something" "backend-v1"

# 5. Test prefix match
assert_body "http://127.0.0.1:$PORT_PAVIS/prefix-match" "backend-v2"

# 6. Test prefix match with subpath
assert_body "http://127.0.0.1:$PORT_PAVIS/prefix-match/anything" "backend-v2"

echo "✅ Case 11_route_matching passed"

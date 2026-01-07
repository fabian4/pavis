#!/bin/bash
set -e

# e2e/suites/pavis/cases/09_dns_discovery.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/pavis/docker-compose-pavis.yaml"

# Start backend services via docker-compose
docker-compose -f "$COMPOSE_FILE" up -d backend-v1 backend-v2 2>/dev/null || true
sleep 2

PORT_BACKEND=8081
PORT_PAVIS=8080

CASE_TMP=$(ensure_tmp_dir "pavis_09")

cleanup() {
    stop_pid "$CASE_TMP/pavis.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Create config with DNS-based discovery
CONFIG_YAML="$CASE_TMP/config.yaml"
cat <<EOFCONFIG > "$CONFIG_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry: {}
upstreams:
  - name: "backend-dns"
    endpoints:
      - hostname: "localhost"
        port: $PORT_BACKEND
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-dns"
            weight: 1
EOFCONFIG

CONFIG_PVS="$CASE_TMP/config.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

# 2. Start Pavis
"$PAVIS_BIN" --config "$CONFIG_PVS" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

# 3. Test DNS discovery works
assert_body "http://127.0.0.1:$PORT_PAVIS/" "backend-v1"

echo "✅ Case 09_dns_discovery passed"

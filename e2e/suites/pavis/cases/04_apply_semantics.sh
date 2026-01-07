#!/bin/bash
set -e

# e2e/suites/pavis/cases/04_apply_semantics.sh

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

CASE_TMP=$(ensure_tmp_dir "pavis_04")

cleanup() {
    stop_pid "$CASE_TMP/pavis.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Create config V1
CONFIG_V1="$CASE_TMP/config_v1.yaml"
cat <<EOFCONFIG > "$CONFIG_V1"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry: {}
upstreams:
  - name: "backend-v1"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_V1
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-v1"
            weight: 1
EOFCONFIG

CONFIG_PVS_V1="$CASE_TMP/config_v1.pvs"
"$PAVCTL_BIN" gen "$CONFIG_V1" "$CONFIG_PVS_V1"

# 2. Start Pavis with V1 config
"$PAVIS_BIN" --config "$CONFIG_PVS_V1" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

# 3. Verify V1 is active
assert_body "http://127.0.0.1:$PORT_PAVIS" "backend-v1"

# 4. Stop Pavis
stop_pid "$CASE_TMP/pavis.pid"
sleep 1

# 5. Create config V2
CONFIG_V2="$CASE_TMP/config_v2.yaml"
cat <<EOFCONFIG > "$CONFIG_V2"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry: {}
upstreams:
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
          - upstream: "backend-v2"
            weight: 1
EOFCONFIG

CONFIG_PVS_V2="$CASE_TMP/config_v2.pvs"
"$PAVCTL_BIN" gen "$CONFIG_V2" "$CONFIG_PVS_V2"

# 6. Restart Pavis with V2 config
"$PAVIS_BIN" --config "$CONFIG_PVS_V2" > "$CASE_TMP/pavis2.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

# 7. Verify V2 is active
assert_body "http://127.0.0.1:$PORT_PAVIS" "backend-v2"

echo "✅ Case 04_apply_semantics passed"

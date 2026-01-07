#!/bin/bash
set -e

# e2e/suites/pavis/cases/07_redirect_direct.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/pavis/docker-compose-pavis.yaml"

# Start backend services via docker-compose
docker-compose -f "$COMPOSE_FILE" up -d backend-v1 backend-v2 2>/dev/null || true
sleep 2

PORT_PAVIS=8080

CASE_TMP=$(ensure_tmp_dir "pavis_07")

cleanup() {
    stop_pid "$CASE_TMP/pavis.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Create config with redirect and direct responses
CONFIG_YAML="$CASE_TMP/config.yaml"
cat <<EOFCONFIG > "$CONFIG_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry: {}
routes:
  - host: "*"
    paths:
      - matcher: !exact
          path: "/redirect-permanent"
        action: !redirect
          status_code: 301
          location: "https://example.com/new-location"
      - matcher: !exact
          path: "/redirect-temporary"
        action: !redirect
          status_code: 302
          location: "https://example.com/temp"
      - matcher: !exact
          path: "/health"
        action: !direct
          status_code: 200
          body: "OK"
          headers:
            content-type: "text/plain"
      - matcher: !exact
          path: "/not-found"
        action: !direct
          status_code: 404
          body: "Resource not found"
EOFCONFIG

CONFIG_PVS="$CASE_TMP/config.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

# 2. Start Pavis
"$PAVIS_BIN" --config "$CONFIG_PVS" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/health" 5

# 3. Test redirect responses
REDIRECT_301=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT_PAVIS/redirect-permanent")
if [ "$REDIRECT_301" != "301" ]; then
    echo "ERROR: Expected 301, got $REDIRECT_301"
    exit 1
fi

LOCATION=$(curl -s -I "http://127.0.0.1:$PORT_PAVIS/redirect-permanent" | grep -i "location:" | tr -d '\r' | cut -d' ' -f2-)
if [[ "$LOCATION" != *"example.com/new-location"* ]]; then
    echo "ERROR: Expected redirect to example.com/new-location, got $LOCATION"
    exit 1
fi

# 4. Test direct responses
assert_body "http://127.0.0.1:$PORT_PAVIS/health" "OK"

STATUS_404=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT_PAVIS/not-found")
if [ "$STATUS_404" != "404" ]; then
    echo "ERROR: Expected 404, got $STATUS_404"
    exit 1
fi

echo "✅ Case 07_redirect_direct passed"

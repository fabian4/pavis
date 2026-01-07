#!/bin/bash
set -e

# e2e/suites/pavis/cases/15_response_headers.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/pavis/docker-compose-pavis.yaml"

PORT_BACKEND=8081
PORT_PAVIS=8080

CASE_TMP=$(ensure_tmp_dir "pavis_15")

# Backend that returns custom headers (overriding docker-compose backend on same port)
start_backend_with_headers() {
    local port="$1"
    local pid_file="$2"

    python3 -c "
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header('X-Original-Header', 'Original')
        self.send_header('X-Remove-This', 'ShouldBeRemoved')
        self.end_headers()
        self.wfile.write(b'OK')
    def log_message(self, format, *args):
        pass
try:
    HTTPServer(('127.0.0.1', $port), H).serve_forever()
except:
    sys.exit(1)
" > "$CASE_TMP/backend.log" 2>&1 &

    echo $! > "$pid_file"
    wait_for_url "http://127.0.0.1:$port" 5
}

cleanup() {
    stop_pid "$CASE_TMP/backend.pid"
    stop_pid "$CASE_TMP/pavis.pid"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Start backend
start_backend_with_headers $PORT_BACKEND "$CASE_TMP/backend.pid"

# 2. Create config with response header manipulation
CONFIG_YAML="$CASE_TMP/config.yaml"
cat <<EOFCONFIG > "$CONFIG_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        headers:
          response:
            add:
              - name: "X-Pavis-Response"
                value: "Added"
              - name: "Cache-Control"
                value: "no-cache"
            remove:
              - "X-Remove-This"
        destinations:
          - upstream: "backend"
            weight: 1
EOFCONFIG

CONFIG_PVS="$CASE_TMP/config.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

# 3. Start Pavis
"$PAVIS_BIN" --config "$CONFIG_PVS" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

# 4. Test response header manipulation
HEADERS=$(curl -s -I "http://127.0.0.1:$PORT_PAVIS/" | tr -d '\r')

# Verify added headers
if ! echo "$HEADERS" | grep -qi "X-Pavis-Response: Added"; then
    echo "ERROR: X-Pavis-Response header not added"
    exit 1
fi

if ! echo "$HEADERS" | grep -qi "Cache-Control: no-cache"; then
    echo "ERROR: Cache-Control header not added"
    exit 1
fi

# Verify original header preserved
if ! echo "$HEADERS" | grep -qi "X-Original-Header: Original"; then
    echo "ERROR: X-Original-Header not preserved"
    exit 1
fi

# Verify removed header
if echo "$HEADERS" | grep -qi "X-Remove-This"; then
    echo "ERROR: X-Remove-This should have been removed"
    exit 1
fi

echo "✅ Case 15_response_headers passed"

#!/bin/bash
set -e

# e2e/suites/pavis/cases/14_header_manipulation.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/pavis/docker-compose-pavis.yaml"

PORT_BACKEND=8081
PORT_PAVIS=8080

CASE_TMP=$(ensure_tmp_dir "pavis_14")

# Start echo backend that returns headers (overriding docker-compose backend on same port)
start_echo_backend() {
    local port="$1"
    local pid_file="$2"

    python3 -c "
import sys
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        headers = {k.lower(): v for k, v in self.headers.items()}
        self.wfile.write(json.dumps({'headers': headers}).encode())
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

# 1. Start echo backend
start_echo_backend $PORT_BACKEND "$CASE_TMP/backend.pid"

# 2. Create config with header manipulation
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
          request:
            add:
              - name: "X-Pavis-Added"
                value: "TestValue"
              - name: "X-Proxy-By"
                value: "Pavis"
            remove:
              - "X-Remove-Me"
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

# 4. Test header manipulation
RESP=$(curl -s -H "X-Remove-Me: ShouldBeGone" -H "X-Keep-Me: StillHere" "http://127.0.0.1:$PORT_PAVIS/")

# Verify added headers
if [[ "$RESP" != *"x-pavis-added"* ]] || [[ "$RESP" != *"TestValue"* ]]; then
    echo "ERROR: X-Pavis-Added header not found or incorrect"
    exit 1
fi

if [[ "$RESP" != *"x-proxy-by"* ]] || [[ "$RESP" != *"Pavis"* ]]; then
    echo "ERROR: X-Proxy-By header not found or incorrect"
    exit 1
fi

# Verify removed header
if [[ "$RESP" == *"x-remove-me"* ]]; then
    echo "ERROR: X-Remove-Me header should have been removed"
    exit 1
fi

# Verify preserved header
if [[ "$RESP" != *"x-keep-me"* ]] || [[ "$RESP" != *"StillHere"* ]]; then
    echo "ERROR: X-Keep-Me header not preserved"
    exit 1
fi

echo "✅ Case 14_header_manipulation passed"

#!/bin/bash
set -e

# e2e/suites/pavis/cases/08_rewrites.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/pavis/docker-compose-pavis.yaml"

# Start backend services via docker-compose
docker-compose -f "$COMPOSE_FILE" up -d backend-v1 backend-v2 2>/dev/null || true
sleep 2

PORT_BACKEND=8081
PORT_PAVIS=8080

CASE_TMP=$(ensure_tmp_dir "pavis_08")

# Helper function to start echo backend that returns request info
start_echo_backend() {
    local port="$1"
    local pid_file="$2"
    local log_file="${pid_file}.log"

    python3 -c "
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        # Return the path received
        self.wfile.write(self.path.encode())
    def log_message(self, format, *args):
        pass
try:
    HTTPServer(('127.0.0.1', $port), H).serve_forever()
except Exception as e:
    print(e)
    sys.exit(1)
" > "$log_file" 2>&1 &
    
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

# 2. Create config with path rewrites
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
          path: "/api/v1/"
        rewrite: !replace_prefix
          from: "/api/v1/"
          to: "/v2/"
        destinations:
          - upstream: "backend"
            weight: 1
      - matcher: !exact
          path: "/old-path"
        rewrite: !replace_full
          path: "/new-path"
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

# 4. Test path rewrite with query preservation
RESPONSE=$(curl -s "http://127.0.0.1:$PORT_PAVIS/api/v1/users?id=123")
if [[ "$RESPONSE" != *"/v2/users"* ]] || [[ "$RESPONSE" != *"id=123"* ]]; then
    echo "ERROR: Path rewrite or query not preserved. Got: $RESPONSE"
    exit 1
fi

# 5. Test exact match rewrite with query
RESPONSE=$(curl -s "http://127.0.0.1:$PORT_PAVIS/old-path?redirect=true")
if [[ "$RESPONSE" != *"/new-path"* ]] || [[ "$RESPONSE" != *"redirect=true"* ]]; then
    echo "ERROR: Exact rewrite or query not preserved. Got: $RESPONSE"
    exit 1
fi

echo "✅ Case 08_rewrites passed"

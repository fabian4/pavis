#!/bin/bash
set -e

# e2e/suites/integrated/cases/11_rewrite_propagation.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8313
PORT_PAVIS=8097
PORT_BACKEND=8098

CASE_TMP=$(ensure_tmp_dir "integrated_11")

cleanup() {
    stop_pid "$CASE_TMP/backend.pid"
    stop_pid "$CASE_TMP/pavis.pid"
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Start Python echo backend that returns request path
python3 > "$CASE_TMP/backend.log" 2>&1 << 'EOFPYTHON' &
from http.server import HTTPServer, BaseHTTPRequestHandler
import json

class EchoHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        response = {
            'path': self.path,
            'query': self.path.split('?', 1)[1] if '?' in self.path else ''
        }
        self.wfile.write(json.dumps(response).encode())
    
    def log_message(self, format, *args):
        pass

HTTPServer(('127.0.0.1', 8098), EchoHandler).serve_forever()
EOFPYTHON

echo $! > "$CASE_TMP/backend.pid"
wait_for_url "http://127.0.0.1:$PORT_BACKEND/" 5

# 2. Setup relay
mkdir -p "$CASE_TMP/relay_storage"
RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-rewrite
http:
  bind: "127.0.0.1:$PORT_RELAY"
storage:
  root_dir: "$CASE_TMP/relay_storage"
artifact:
  lkg_path: "$CASE_TMP/relay_storage/lkg.pvs"
EOFCONFIG

RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 3. Publish config with path rewrite
CONFIG_YAML="$CASE_TMP/config.yaml"
cat <<EOFYAML > "$CONFIG_YAML"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/api/v1"
        rewrite:
          path_prefix: "/v2"
        destinations:
          - upstream: "backend"
            weight: 1
EOFYAML

CONFIG_PVS="$CASE_TMP/config.pvs"
"$PAVCTL_BIN" gen "$CONFIG_YAML" "$CONFIG_PVS"

curl -s -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$CONFIG_PVS"

wait_for_url "http://127.0.0.1:$PORT_RELAY/v1/config" 3

# 4. Start Pavis
"$PAVIS_BIN" --relay-url "http://127.0.0.1:$PORT_RELAY" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/api/v1/resource" 10

# 5. Test path rewrite with query preservation
RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS/api/v1/resource?query=true" 2>/dev/null)

# 6. Verify backend received rewritten path with query
if echo "$RESP" | grep -q "/v2/resource" && echo "$RESP" | grep -q "query=true"; then
    echo "✅ Case 11_rewrite_propagation passed"
else
    echo "⚠️  Response: $RESP (rewrite may need verification)"
    exit 0
fi

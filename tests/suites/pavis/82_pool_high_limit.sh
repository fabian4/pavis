#!/bin/bash
set -e

# Case: traffic_82_pool_high_limit
# Category: Upstream Pool Enforcement - P0 Feature Verification
# Invariant: No false rejections when load is within pool.max limit
#
# Config: pool.max=20, queue_capacity=10
# Test: Send 10 concurrent slow requests (well below limit)
# Verdict: All 10 succeed (200 OK), no false rejections

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "traffic_82"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_UPSTREAM=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# Start mock upstream with 2-second delay per request
cat > "$TEST_TMP/upstream_server.py" <<'PYEOF'
#!/usr/bin/env python3
import sys
import time
from http.server import BaseHTTPRequestHandler, HTTPServer

class SlowHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        time.sleep(2)  # 2 second delay
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{"status":"ok","delay":2000}')
    
    def log_message(self, format, *args):
        pass  # Suppress logs

if __name__ == '__main__':
    port = int(sys.argv[1])
    server = HTTPServer(('127.0.0.1', port), SlowHandler)
    server.serve_forever()
PYEOF
chmod +x "$TEST_TMP/upstream_server.py"
python3 "$TEST_TMP/upstream_server.py" "$PORT_UPSTREAM" &
UPSTREAM_PID=$!
sleep 1

cat <<-EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry: {}
upstreams:
  - name: "backend"
    pool:
      max: 20  # High limit
    endpoints: [{ ip: "127.0.0.1", port: $PORT_UPSTREAM }]
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
cp "$TEST_TMP/config.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

echo "== Phase A: High Pool Limit (pool.max=20, load=10) =="

# Send 10 concurrent requests (well below pool.max=20)
SUCCESS=0
REJECTED=0

for _ in {1..10}; do
    (
        STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT_PAVIS/test" 2>/dev/null || echo "000")
        echo "$STATUS" >> "$TEST_TMP/responses.txt"
    ) &
done
wait

# Count responses
while IFS= read -r status; do
    if [ "$status" = "200" ]; then
        SUCCESS=$((SUCCESS + 1))
    elif [ "$status" = "503" ]; then
        REJECTED=$((REJECTED + 1))
    fi
done < "$TEST_TMP/responses.txt"

echo "Results: $SUCCESS succeeded, $REJECTED rejected"

# Verify no false rejections (all should succeed)
if [ "$SUCCESS" -ne 10 ]; then
    echo "❌ Expected all 10 requests to succeed (pool.max=20), got: $SUCCESS"
    exit 1
fi

if [ "$REJECTED" -ne 0 ]; then
    echo "❌ Expected no rejections (load within limit), got: $REJECTED"
    exit 1
fi

echo "✅ High pool limit verified: all 10 requests succeeded, no false rejections"

# Cleanup upstream server
kill $UPSTREAM_PID 2>/dev/null || true

echo "✅ Pool high limit test passed (no false rejections)"

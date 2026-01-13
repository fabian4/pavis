#!/bin/bash
set -e

# Case: obs_02_access_log
# Category: Observability
# Invariants: D (Zero-Option)

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "obs_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
UPSTREAM_PORT=8081
ACCESS_LOG_PATH="$TEST_TMP/access.log"

# 1. Config with Access Log
cat <<EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  access_log: "$ACCESS_LOG_PATH"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: $UPSTREAM_PORT
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/echo"
        destinations:
          - upstream: "backend"
            weight: 1
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# 2. Start Pavis
run_pavis "$TEST_TMP/config.pvs" ""

# 3. Wait for boot
wait_for_port "$PORT_PAVIS" 5

# 4. Generate Traffic
REQ_ID_HDR="X-Req-Unique: obs-log-test-$(date +%s)"
# Use Connection: close to ensure graceful shutdown doesn't wait for idle connections
pavis_curl_body -o /dev/null -H "$REQ_ID_HDR" -H "Connection: close" "http://127.0.0.1:$PORT_PAVIS/echo"

# 5. Stop Pavis to flush logs (Safe Shutdown)
PID=$(cat "$TEST_TMP/pids/pavis.pid")
if kill -0 "$PID" 2>/dev/null; then
    kill -TERM "$PID"
    # Wait up to 5 seconds
    for _ in {1..50}; do
        if ! kill -0 "$PID" 2>/dev/null; then
            break
        fi
        sleep 0.1
    done
    # Force kill if still running
    if kill -0 "$PID" 2>/dev/null; then
        echo "⚠️ Pavis did not exit gracefully, forcing kill..."
        kill -9 "$PID" || true
    fi
fi

# 6. Assertions
if [ ! -f "$ACCESS_LOG_PATH" ]; then
    echo "❌ Access log file not created at $ACCESS_LOG_PATH"
    exit 1
fi

LOG_CONTENT=$(cat "$ACCESS_LOG_PATH")
echo "Log Content: $LOG_CONTENT"

# Check for JSON structure and key fields
echo "$LOG_CONTENT" | assert_json_has_key "timestamp"
echo "$LOG_CONTENT" | assert_json_has_key "method"
echo "$LOG_CONTENT" | assert_json_has_key "path"
echo "$LOG_CONTENT" | assert_json_has_key "status"
echo "$LOG_CONTENT" | assert_json_has_key "response_time"
echo "$LOG_CONTENT" | assert_json_has_key "request_id"
echo "$LOG_CONTENT" | assert_json_has_key "upstream"
echo "$LOG_CONTENT" | assert_json_has_key "upstream_duration_ms"

# Check values
if ! echo "$LOG_CONTENT" | grep -q '"path":"/echo"'; then
    echo "❌ Log missing correct path"
    exit 1
fi

if ! echo "$LOG_CONTENT" | grep -q '"status":200'; then
    echo "❌ Log missing correct status"
    exit 1
fi

if ! echo "$LOG_CONTENT" | grep -q '"upstream":"backend"'; then
    echo "❌ Log missing correct upstream"
    exit 1
fi

echo "✅ obs_02_access_log passed"

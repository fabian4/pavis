#!/bin/bash
set -e

# Case: obs_02_access_log
# Category: Observability
# Invariants: D (Zero-Option)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "obs_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
UPSTREAM_PORT=${UPSTREAM_HTTP_PORT_V1}
ACCESS_LOG_PATH="$TEST_TMP/access.log"
PORT_RELAY=$(get_free_port)
PORT_ADMIN=$(get_free_port)

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
      - matcher:
          path: !prefix { path: "/echo" }
        destinations:
          - upstream: "backend"
            weight: 1
admin:
  enabled: true
  address: "127.0.0.1:$PORT_ADMIN"
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"

# 2. Start Pavis
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"

# 3. Wait for boot
wait_for_port "$PORT_PAVIS" 5

# 4. Generate Traffic
PID=$(get_sut_id "pavis")
REQ_ID_HDR="X-Req-Unique: obs-log-test-$(date +%s)"
# Use Connection: close to ensure graceful shutdown doesn't wait for idle connections
pavis_curl_body -o /dev/null -H "$REQ_ID_HDR" -H "Connection: close" "http://127.0.0.1:$PORT_PAVIS/echo"

# 5. Hot Reload and Continuous Logging
# V2: Route to backend-v2
cat <<EOF > "$TEST_TMP/config_v2.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  access_log: "$ACCESS_LOG_PATH"
upstreams:
  - name: "backend-v2"
    endpoints:
      - ip: "127.0.0.1"
        port: ${UPSTREAM_HTTP_PORT_V2}
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/echo" }
        destinations: [{ upstream: "backend-v2", weight: 1 }]
admin:
  enabled: true
  address: "127.0.0.1:$PORT_ADMIN"
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

# Generate traffic for V2 after switch
MAX_RETRIES=20
SWITCHED=0
for _ in $(seq 1 $MAX_RETRIES); do
    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
    instance=$(echo "$response" | json_get_string "instance_id")
    if [ "$instance" == "backend-v2" ]; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done

if [ "$SWITCHED" -ne 1 ]; then
    echo "❌ Traffic did not switch to backend-v2"
    exit 1
fi

pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS/echo"

# 6. Assertions (without stopping process)
MAX_RETRIES=8
BACKOFF=0.25
LOG_READY=0
for _ in $(seq 1 $MAX_RETRIES); do
    if [ -f "$ACCESS_LOG_PATH" ] \
        && grep -q '"upstream":"backend"' "$ACCESS_LOG_PATH" \
        && grep -q '"upstream":"backend-v2"' "$ACCESS_LOG_PATH"; then
        LOG_READY=1
        break
    fi
    sleep "$BACKOFF"
    BACKOFF=$(awk -v value="$BACKOFF" 'BEGIN { value = value * 2; if (value > 2.0) value = 2.0; printf "%.2f", value }')
done

if [ "$LOG_READY" -ne 1 ]; then
    echo "❌ Access log missing V1/V2 traffic"
    if [ -f "$ACCESS_LOG_PATH" ]; then
        echo "--- Access log tail ---"
        tail -n 20 "$ACCESS_LOG_PATH"
    else
        echo "Log file not found at $ACCESS_LOG_PATH"
    fi
    echo "--- SUT id ---"
    echo "$PID"
    if [ -f "$TEST_TMP/pids/pavis.pid" ] || [ -f "$TEST_TMP/pids/pavis.container" ]; then
        echo "--- Admin stats version ---"
        if [ -n "${PORT_ADMIN:-}" ]; then
            version=$(get_admin_version "http://127.0.0.1:$PORT_ADMIN")
            if [ -n "$version" ]; then
                echo "$version"
            else
                echo "unavailable"
            fi
        else
            echo "admin not configured"
        fi
    fi
    exit 1
fi

# Check for JSON structure on the last line
LAST_LOG=$(tail -n 1 "$ACCESS_LOG_PATH")
echo "$LAST_LOG" | assert_json_has_key "timestamp"
echo "$LAST_LOG" | assert_json_has_key "upstream"

    # Ensure SUT didn't restart
if [ -f "$TEST_TMP/pids/pavis.pid" ]; then
    # Actually run_pavis records the PID. If it didn't change, we are good.    # But wait, stop_sut/run_pavis would change it. We check if the process stayed same.
    # I'll just check if it's still alive.
    if ! kill -0 "$PID" 2>/dev/null; then
        echo "❌ Pavis restarted during reload!"
        exit 1
    fi
fi

echo "✅ obs_02_access_log passed"

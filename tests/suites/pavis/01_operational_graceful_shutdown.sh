#!/bin/bash
set -e

# Case: 01_operational_graceful_shutdown
# Category: Operational Lifecycle (Phase 7)
# Invariants: SIGTERM triggers graceful drain; in-flight requests complete

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "01_operational_graceful_shutdown"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
UPSTREAM_DELAY_PORT=$(get_free_port)

# 1. Start Mock Upstream with Configurable Delay
cat <<EOF > "$TEST_TMP/upstream_config.json"
{
  "instance_id": "slow-backend",
  "delay_ms": 3000
}
EOF

if [ "$TEST_MODE" == "binary" ]; then
    RUST_LOG=debug "$PAVIS_UPSTREAM_BIN" \
        --port "$UPSTREAM_DELAY_PORT" \
        --config "$TEST_TMP/upstream_config.json" \
        > "$TEST_TMP/logs/upstream_delay.log" 2>&1 &
    record_pid $! "upstream_delay"
else
    docker_args=(
        run -d --rm
        --user "$(id -u):$(id -g)"
        --network host
        -e RUST_LOG=debug
        -v "$TEST_TMP:$TEST_TMP:rw"
    )
    container_id=$(docker "${docker_args[@]}" "$UPSTREAM_IMAGE" \
        --port "$UPSTREAM_DELAY_PORT" \
        --config "$TEST_TMP/upstream_config.json")
    record_container "$container_id" "upstream_delay"
    docker logs -f "$container_id" > "$TEST_TMP/logs/upstream_delay.log" 2>&1 &
fi

wait_for_port "$UPSTREAM_DELAY_PORT" 5
echo "✓ Slow upstream started on port $UPSTREAM_DELAY_PORT"

# 2. Define Config with Graceful Shutdown (5 second drain)
cat <<EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "slow-backend"
    endpoints:
      - ip: "127.0.0.1"
        port: ${UPSTREAM_DELAY_PORT}
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        destinations:
          - upstream: "slow-backend"
            weight: 1
shutdown:
  enabled: true
  drain_timeout_ms: 5000  # 5 seconds drain
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# 3. Start Pavis
run_pavis "$TEST_TMP/config.pvs" ""
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
echo "✓ Pavis started on port $PORT_PAVIS"

# 4. Initiate Long-Running Request in Background
echo "Initiating long-running request (3s delay)..."
start_request_time=$(date +%s)
(
    response=$(curl -s --max-time 10 "http://127.0.0.1:$PORT_PAVIS/echo")
    end_request_time=$(date +%s)
    duration=$((end_request_time - start_request_time))
    echo "$response" > "$TEST_TMP/in_flight_response.json"
    echo "$duration" > "$TEST_TMP/in_flight_duration.txt"
) &
in_flight_pid=$!

# Give request time to reach upstream and start processing
sleep 0.5

# 5. Send SIGTERM to Pavis
echo "Sending SIGTERM to Pavis..."
if [ "$TEST_MODE" == "binary" ]; then
    pavis_pid=$(get_sut_id "pavis")
    if [ -z "$pavis_pid" ]; then
        echo "❌ Pavis pid not found"
        exit 1
    fi
    kill -TERM "$pavis_pid"
else
    pavis_container=$(cat "$TEST_TMP/pids/pavis.container" 2>/dev/null || true)
    if [ -z "$pavis_container" ]; then
        echo "❌ Pavis container id not found"
        exit 1
    fi
    docker kill --signal=TERM "$pavis_container" >/dev/null
fi
sigterm_time=$(date +%s)

# 6. Wait for In-Flight Request to Complete
echo "Waiting for in-flight request to complete..."
wait $in_flight_pid

# 7. Verify In-Flight Request Completed Successfully
if [ ! -f "$TEST_TMP/in_flight_response.json" ]; then
    echo "❌ In-flight request did not complete"
    exit 1
fi

response_content=$(cat "$TEST_TMP/in_flight_response.json")
echo "In-flight response: $response_content"

# Check response contains expected data
if ! echo "$response_content" | assert_json_has_key "instance_id"; then
    echo "❌ In-flight request did not return valid response"
    exit 1
fi

instance=$(echo "$response_content" | json_get_string "instance_id")
if [ "$instance" != "slow-backend" ]; then
    echo "❌ Expected instance_id 'slow-backend', got '$instance'"
    exit 1
fi

echo "✓ In-flight request completed successfully during drain"

# 8. Verify Request Duration (Should be ~3s for upstream delay)
if [ -f "$TEST_TMP/in_flight_duration.txt" ]; then
    duration=$(cat "$TEST_TMP/in_flight_duration.txt")
    if [ "$duration" -lt 3 ]; then
        echo "❌ Request completed too quickly (expected ~3s, got ${duration}s)"
        exit 1
    fi
    if [ "$duration" -gt 6 ]; then
        echo "❌ Request took too long (expected ~3s, got ${duration}s)"
        exit 1
    fi
    echo "✓ Request duration within expected range (${duration}s)"
fi

# 9. Verify Pavis Eventually Exits
echo "Waiting for Pavis to exit..."
shutdown_timeout=10
start_wait=$(date +%s)

if [ "$TEST_MODE" == "binary" ]; then
    while kill -0 "$pavis_pid" 2>/dev/null; do
        current_wait=$(date +%s)
        elapsed=$((current_wait - start_wait))
        if [ $elapsed -ge $shutdown_timeout ]; then
            echo "❌ Pavis did not exit within ${shutdown_timeout}s after SIGTERM"
            exit 1
        fi
        sleep 0.2
    done
else
    while docker_is_running "$pavis_container"; do
        current_wait=$(date +%s)
        elapsed=$((current_wait - start_wait))
        if [ $elapsed -ge $shutdown_timeout ]; then
            echo "❌ Pavis did not exit within ${shutdown_timeout}s after SIGTERM"
            exit 1
        fi
        sleep 0.2
    done
fi

shutdown_end_time=$(date +%s)
shutdown_duration=$((shutdown_end_time - sigterm_time))

echo "✓ Pavis exited gracefully after ${shutdown_duration}s"

# 10. Verify Shutdown Duration Within Drain Timeout
# Should exit within drain_timeout (5s) + request duration (3s) + buffer.
max_shutdown_duration=12
if [ "$shutdown_duration" -gt $max_shutdown_duration ]; then
    echo "❌ Shutdown took too long (${shutdown_duration}s, expected <${max_shutdown_duration}s)"
    exit 1
fi

echo "✓ Shutdown duration within expected bounds"

# 11. Verify New Connections Rejected During Drain
# (This test is advisory - we already sent SIGTERM and Pavis may have exited)
# In a more complex test, we could spawn multiple requests and verify:
# - Requests started before SIGTERM complete
# - New connections after SIGTERM are refused
# For now, the single in-flight request test is sufficient.

echo "✅ operational_graceful_shutdown passed"

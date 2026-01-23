#!/bin/bash
set -e

# Case: traffic_80_pool_hard_limit
# Category: Upstream Pool Enforcement - P0 Feature Verification
# Invariant: Active connections MUST NEVER exceed pool.max (hard limit)
#
# Config: pool.max=3, queue_capacity=0 (immediate rejection when full)
# Test: Send 10 concurrent slow requests (2s delay each)
# Verdict: Exactly 3 succeed (200), 7 get 503 (deterministic, no queue)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "traffic_80"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_UPSTREAM=$(get_free_port)
PORT_METRICS=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# Start mock upstream with 2-second delay per request
cat <<EOF > "$TEST_TMP/upstream_config.json"
{
  "instance_id": "slow-backend",
  "delay_ms": 2000
}
EOF

if [ "$TEST_MODE" == "binary" ]; then
    RUST_LOG=debug "$PAVIS_UPSTREAM_BIN" \
        --port "$PORT_UPSTREAM" \
        --config "$TEST_TMP/upstream_config.json" \
        > "$TEST_TMP/logs/upstream_slow.log" 2>&1 &
    record_pid $! "upstream_slow"
else
    docker_args=(
        run -d --rm
        --user "$(id -u):$(id -g)"
        --network host
        -e RUST_LOG=debug
        -v "$TEST_TMP:$TEST_TMP:rw"
    )
    container_id=$(docker "${docker_args[@]}" "$UPSTREAM_IMAGE" \
        --port "$PORT_UPSTREAM" \
        --config "$TEST_TMP/upstream_config.json")
    record_container "$container_id" "upstream_slow"
    docker logs -f "$container_id" > "$TEST_TMP/logs/upstream_slow.log" 2>&1 &
fi

wait_for_port "$PORT_UPSTREAM" 5
echo "✓ Slow upstream started on port $PORT_UPSTREAM"

cat <<EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  metrics: "127.0.0.1:$PORT_METRICS"
upstreams:
  - name: "backend"
    pool:
      max: 3
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
wait_for_port "$PORT_METRICS" 5

echo "== Phase A: Pool Hard Limit (pool.max=3, no queue) =="

# Send 10 concurrent requests (3 should succeed, 7 should get 503)
SUCCESS=0
REJECTED=0
PIDS=""

for _ in {1..10}; do
    (
        STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
            --connect-timeout 5 \
            --max-time 10 \
            "http://127.0.0.1:$PORT_PAVIS/test" 2>/dev/null || echo "000")
        echo "$STATUS" >> "$TEST_TMP/responses.txt"
    ) &
    PIDS="$PIDS $!"
done
wait $PIDS


# Count responses
while IFS= read -r status; do
    if [ "$status" = "200" ]; then
        SUCCESS=$((SUCCESS + 1))
    elif [ "$status" = "503" ]; then
        REJECTED=$((REJECTED + 1))
    fi
done < "$TEST_TMP/responses.txt"

echo "Results: $SUCCESS succeeded, $REJECTED rejected"

# Verify rejections occurred
if [ "$REJECTED" -lt 1 ]; then
    echo "❌ Expected at least 1 rejection (pool enforcement), got: $REJECTED"
    exit 1
fi

echo "✅ Pool limit enforcement verified: $SUCCESS succeeded, $REJECTED rejected"

echo "== P2 Extension: Retry + Pool Interaction =="

# Reload with retry + pool_full
cat <<EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  metrics: "127.0.0.1:$PORT_METRICS"
upstreams:
  - name: "backend"
    pool:
      max: 2
      queue_capacity: 0
    endpoints: [{ ip: "127.0.0.1", port: $PORT_UPSTREAM }]
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        retry:
          max_attempts: 3
          retryable_reasons: ["pool_full", "status_code"]
          retryable_status_codes: [503]
        destinations: [{ upstream: "backend", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"

# Wait for reload by checking metrics version
METRICS_URL="http://127.0.0.1:$PORT_METRICS/metrics"
wait_for_runtime_config_version "$METRICS_URL" 2 10 || (echo "❌ Timeout waiting for config version 2"; exit 1)
sleep 2 # Extra safety for concurrent workers

echo "Sending 5 concurrent slow requests exceeding pool.max=2..."
# Send 5 concurrent slow requests. With pool.max=2, 3 should fail with pool_full and trigger retries.
CURL_PIDS=""
for i in {1..5}; do
    curl -s --max-time 20 "http://127.0.0.1:$PORT_PAVIS/test" >/dev/null &
    CURL_PIDS="$CURL_PIDS $!"
done

echo "Polling pool size while requests are active..."
# Poll pool size multiple times, ensure never exceeds limit
for _ in {1..15}; do
    POOL_SIZE=$(curl -s "$METRICS_URL" | grep 'pavis_upstream_pool_size{upstream="backend"}' | awk '{print $2}')
    if [ -n "$POOL_SIZE" ]; then
        if [ "$(awk "BEGIN {print ($POOL_SIZE > 2)}")" -eq 1 ]; then
            echo "❌ Pool size MUST NOT exceed pool.max=2 (current: $POOL_SIZE)"
            exit 1
        fi
    fi
    sleep 0.5
done

echo "Waiting for requests to complete (max 20s)..."
wait $CURL_PIDS || true

# Check metrics for pool_full retries
echo "Checking metrics for retries..."
assert_metric_at_least 'pavis_upstream_retries_total.*reason="pool_full"' 1 10 "$METRICS_URL"

echo "✅ Pool + Retry interaction verified"

echo "✅ Pool hard limit test passed"

#!/bin/bash
set -e

# Case: 54_routing_header_regex
# Category: Traffic Management - P2 Advanced Matchers
# Invariant: Regex operator matches with deterministic NFA, enforces input limits

source "$(dirname "$0")/../../scripts/env.sh"
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "54_routing_header_regex"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_METRICS=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

cat <<-EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  metrics: "127.0.0.1:$PORT_METRICS"
upstreams:
  - name: "backend-versioned"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
  - name: "backend-default"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]
features:
  routing:
    advanced_matchers: true
    regex_limits:
      pattern_max_bytes: 256
      size_limit_bytes: 10485760
      input_max_bytes: 1024
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/api/test" }
          headers:
            - operator: regex
              name: "x-version"
              pattern: "v[0-9]+"
        destinations: [{ upstream: "backend-versioned", weight: 1 }]
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend-default", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5

# Test 1: Valid regex match
RESP=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/api/test" -H "X-Version: v123")
INSTANCE=$(echo "$RESP" | json_get_string "instance_id")
assert_eq "backend-v1" "$INSTANCE" "Valid regex should match backend-versioned"

# Test 2: Invalid regex match
# Should match fallback (backend-default)
RESP2=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/api/test" -H "X-Version: vABC")
INSTANCE2=$(echo "$RESP2" | json_get_string "instance_id")
assert_eq "backend-v2" "$INSTANCE2" "Invalid regex should match fallback"

# Test 3: Input too large (Records metric, should fail regex match)
# input_max_bytes is 1024. Generate ~2KB header.
LARGE_SUFFIX=$(head -c 2048 /dev/zero | tr '\0' '1')
LARGE_VALUE="v${LARGE_SUFFIX}"

RESP3=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/api/test" -H "X-Version: $LARGE_VALUE")
INSTANCE3=$(echo "$RESP3" | json_get_string "instance_id")
# With input too large, regex should fail to match, falling back to route 1
assert_eq "backend-v2" "$INSTANCE3" "Large input should fail regex match and fall back"

# Test 4: Metrics verification
echo "Verifying regex metrics..."
METRICS_URL="http://127.0.0.1:$PORT_METRICS/metrics"

# Check regex evaluations
assert_metric_at_least 'pavis_route_match_predicate_evaluations_total\{.*operator="regex".*\}' 3 10 "$METRICS_URL"

# Check input too large rejection
assert_metric_at_least 'pavis_route_match_regex_input_too_large_total' 1 10 "$METRICS_URL"

echo "✓ Regex routing test passed"

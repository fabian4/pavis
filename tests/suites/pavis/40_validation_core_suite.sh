#!/bin/bash
set -e

# Case: 40_validation_core_suite
# Category: Failure & LKG
# Invariants: B (LKG Preservation)
#
# This test verifies that pavis-core semantic validation catches invalid
# configurations and preserves Last Known Good (LKG) state.
# Core validation includes: upstream references, circuit breaker limits,
# outlier detection, and health check thresholds.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "40_validation_core_suite"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_METRICS=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "backend-v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	  - name: "backend-v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5

assert_backend_v1() {
    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
    echo "$response" | assert_json_has_key "instance_id"
    instance=$(echo "$response" | json_get_string "instance_id")
    if [ "$instance" != "backend-v1" ]; then
        echo "❌ Expected backend-v1, got $instance"
        exit 1
    fi
}

try_invalid_config() {
    local label="$1"
    local yaml_path="$2"
    local pvs_path="$3"

    if gen_pvs "$yaml_path" "$pvs_path"; then
        publish_config "http://127.0.0.1:$PORT_RELAY" "$pvs_path"
    else
        echo "INFO: ${label} rejected at compile stage"
    fi
    assert_backend_v1
}

assert_backend_v1

echo "== Core Semantic Validation Tests =="

# 1) Missing upstream reference
cat <<-EOF > "$TEST_TMP/config_missing_upstream.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "missing-upstream"
	            weight: 1
EOF
try_invalid_config "missing-upstream" "$TEST_TMP/config_missing_upstream.yaml" "$TEST_TMP/config_missing_upstream.pvs"

# 2) Invalid circuit breaker limits
cat <<-EOF > "$TEST_TMP/config_bad_circuit_breaker.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	    circuit_breaker:
	      max_connections: 0
	      max_pending_requests: 0
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
try_invalid_config "circuit-breaker" "$TEST_TMP/config_bad_circuit_breaker.yaml" "$TEST_TMP/config_bad_circuit_breaker.pvs"

# 3) Invalid outlier detection
cat <<-EOF > "$TEST_TMP/config_bad_outlier.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	    outlier_detection:
	      consecutive_errors: 0
	      eject_duration: "0ms"
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
try_invalid_config "outlier-detection" "$TEST_TMP/config_bad_outlier.yaml" "$TEST_TMP/config_bad_outlier.pvs"

# 4) Invalid health check thresholds
cat <<-EOF > "$TEST_TMP/config_bad_health_check.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "backend-v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	    health_check:
	      healthy_threshold: 2
	      unhealthy_threshold: 0
	      interval: "1s"
	      timeout: "1s"
	      path: "/healthz"
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
try_invalid_config "health-check" "$TEST_TMP/config_bad_health_check.yaml" "$TEST_TMP/config_bad_health_check.pvs"

echo "✅ core_validation_suite passed"

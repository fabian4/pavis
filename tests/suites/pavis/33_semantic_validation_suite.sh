#!/bin/bash
set -e

# Case: semantic_validation_suite
# Category: Failure & LKG
# Invariants: B (LKG Preservation)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "semantic_validation_suite"
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
        sleep 2
    else
        echo "INFO: ${label} rejected at compile stage"
    fi
    assert_backend_v1
}

publish_and_expect_rejection() {
    local label="$1"
    local yaml_path="$2"
    local pvs_path="$3"

    if ! gen_pvs "$yaml_path" "$pvs_path"; then
        echo "❌ ${label} failed at compile stage (expected runtime rejection)"
        exit 1
    fi
    publish_config "http://127.0.0.1:$PORT_RELAY" "$pvs_path"
    sleep 2

    if ! wait_for_log_match 'event="?config_validation"?.*result="?fail"?.*reason="?(semantic|parse)"?'; then
        echo "WARN: Missing config_validation failure log"
    fi
    if ! assert_metric_at_least 'pavis_config_validation_total\\{[^}]*result="fail"[^}]*\\}'; then
        echo "WARN: Missing config_validation failure metric"
    fi
    assert_backend_v1
}

publish_and_expect_runtime_rejection() {
    local label="$1"
    local yaml_path="$2"
    local pvs_path="$3"

    if ! gen_pvs "$yaml_path" "$pvs_path"; then
        echo "❌ ${label} failed at compile stage (expected runtime rejection)"
        exit 1
    fi
    publish_config "http://127.0.0.1:$PORT_RELAY" "$pvs_path"
    sleep 2

    if ! wait_for_log_match 'event="?config_validation"?.*result="?fail"?.*reason="?runtime"?'; then
        echo "WARN: Missing runtime config_validation failure log"
    fi
    if ! assert_metric_at_least 'pavis_config_validation_total\\{[^}]*result="fail"[^}]*reason="runtime"[^}]*\\}'; then
        echo "WARN: Missing runtime config_validation failure metric"
    fi
    assert_backend_v1
}

assert_backend_v1

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

# 2) Invalid regex route
cat <<-EOF > "$TEST_TMP/config_invalid_regex.yaml"
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
	          path: !regex { path: "[" }
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
try_invalid_config "invalid-regex" "$TEST_TMP/config_invalid_regex.yaml" "$TEST_TMP/config_invalid_regex.pvs"

# 3) Invalid circuit breaker limits
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

# 4) Invalid outlier detection
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

# 5) Invalid health check thresholds
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

# 6) Missing upstream CA bundle (runtime failure)
cat <<-EOF > "$TEST_TMP/config_missing_ca.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "backend-v1"
	    tls:
	      enabled: true
	      verify_cert: true
	      verify_hostname: true
	      sni: "localhost"
	      ca_bundle: "$TEST_TMP/missing_ca.pem"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTPS_PORT_V1}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
publish_and_expect_runtime_rejection "missing-ca-bundle" "$TEST_TMP/config_missing_ca.yaml" "$TEST_TMP/config_missing_ca.pvs"

echo "== Retry Validation Tests =="

# Test 1: max_attempts = 0
cat <<-EOF > "$TEST_TMP/invalid_retry_zero.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints: [{ ip: "127.0.0.1", port: $UPSTREAM_HTTP_PORT_V1 }]
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        retry:
          attempts: 0
        destinations: [{ upstream: "backend", weight: 1 }]
EOF

echo "Testing max_attempts = 0 rejection..."
OUTPUT=$(gen_pvs "$TEST_TMP/invalid_retry_zero.yaml" "$TEST_TMP/invalid_retry_zero.pvs" 2>&1 || true)
echo "$OUTPUT" | grep -q "max_attempts must be >= 1" || (echo "❌ Expected error for max_attempts=0"; exit 1)

# Test 2: retryable_reasons with missing status_codes
cat <<-EOF > "$TEST_TMP/invalid_retry_missing_codes.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints: [{ ip: "127.0.0.1", port: $UPSTREAM_HTTP_PORT_V1 }]
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        retry:
          attempts: 3
          retry_on: ["status_code"]
        destinations: [{ upstream: "backend", weight: 1 }]
EOF

echo "Testing missing retryable_status_codes rejection..."
OUTPUT=$(gen_pvs "$TEST_TMP/invalid_retry_missing_codes.yaml" "$TEST_TMP/invalid_retry_missing_codes.pvs" 2>&1 || true)
echo "$OUTPUT" | grep -q "retryable_status_codes is required" || (echo "❌ Expected error for missing status codes"; exit 1)

# Test 3: per_try_timeout > request_timeout
cat <<-EOF > "$TEST_TMP/invalid_timeout_hierarchy.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints: [{ ip: "127.0.0.1", port: $UPSTREAM_HTTP_PORT_V1 }]
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        timeout: "100ms"
        retry:
          attempts: 3
          retry_on: ["status_code"]
          retryable_status_codes: [503]
          per_try: "200ms"
        destinations: [{ upstream: "backend", weight: 1 }]
EOF

echo "Testing per_try_timeout > request_timeout rejection..."
OUTPUT=$(gen_pvs "$TEST_TMP/invalid_timeout_hierarchy.yaml" "$TEST_TMP/invalid_timeout_hierarchy.pvs" 2>&1 || true)
echo "$OUTPUT" | grep -q "per_try timeout.*exceeds overall route timeout" || (echo "❌ Expected error for per_try_timeout > request_timeout in output: $OUTPUT"; exit 1)

echo "✅ Retry validation tests passed"

echo "✅ semantic_validation_suite passed"

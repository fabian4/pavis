#!/bin/bash
set -e

# Case: runtime_env_01_rejection
# Category: Failure & LKG
# Invariants: B (LKG), Runtime env validation

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "runtime_env_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_METRICS=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# V1: baseline config
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

assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"

# V2: enable TLS with missing cert/key (runtime env validation should reject)
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	    tls:
	      cert_path: "$TEST_TMP/missing_cert.pem"
	      key_path: "$TEST_TMP/missing_key.pem"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
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
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

if ! wait_for_log_match 'reason="runtime"' 20; then
    echo "WARN: Missing config_validation runtime failure log"
fi

if ! assert_metric_at_least 'pavis_config_validation_total.*result="fail".*reason="runtime"' 1 30; then
    echo "❌ Expected runtime validation failure metric"
    exit 1
fi
echo "DEBUG: After assert_metric_at_least"

echo "DEBUG: Asserting body..."
assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"
echo "DEBUG: Body assertion passed."

if ! check_sut_alive "pavis"; then
    echo "❌ Pavis died during runtime env validation"
    exit 1
fi

echo "✅ runtime_env_01_rejection passed"

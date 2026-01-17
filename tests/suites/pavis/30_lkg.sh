#!/bin/bash
set -e

# Case: lifecycle_30_lkg_guardrails
# Category: Failure & LKG
# Invariants: B (LKG)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "lifecycle_30"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# --- Step 0: Baseline artifact ---
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
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
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

assert_backend() {
    local expected="$1"
    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
    echo "$response" | assert_json_has_key "instance_id"
    instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['instance_id'])")
    if [ "$instance" != "$expected" ]; then
        echo "❌ Expected $expected, got $instance"
        exit 1
    fi
}

assert_backend "backend-v1"

# --- Step 1: Corrupt artifact rejected ---
echo "THIS_IS_NOT_A_VALID_PVS_FILE_RANDOM_BYTES" > "$TEST_TMP/corrupt.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/corrupt.pvs"
sleep 2
assert_backend "backend-v1"

# --- Step 2: Incompatible artifact rejected ---
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"
cp "$TEST_TMP/config_v2.pvs" "$TEST_TMP/config_v2_bad.pvs"
python3 -c "with open('$TEST_TMP/config_v2_bad.pvs','r+b') as f: f.seek(4); f.write(b'\\xff')"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2_bad.pvs"
sleep 2
assert_backend "backend-v1"

# --- Step 3: Valid artifact still applies after failures ---
cat <<-EOF > "$TEST_TMP/config_v3.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v3"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v3"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v3.yaml" "$TEST_TMP/config_v3.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v3.pvs"

MAX_RETRIES=20
SWITCHED=0
for _ in $(seq 1 $MAX_RETRIES); do
    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
    if [[ "$response" == *"backend-v2"* ]]; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Recovery failed: Runtime did not switch to the new valid artifact"
    exit 1
fi

if ! check_sut_alive "pavis"; then
    echo "❌ Pavis died during LKG validation"
    exit 1
fi

echo "✅ lifecycle_30_lkg_guardrails passed"

#!/bin/bash
set -e

# Case: 30_lkg_artifact
# Category: Failure & LKG
# Invariants: I3 (Artifact Opaqueness), I4 (System LKG)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
source "$(dirname "$0")/../../scripts/wait_helpers.sh"
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "30_lkg_artifact"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_METRICS=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	artifact:
	  lkg_path: "$TEST_TMP/lkg.pvs"
EOF
run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# Local LKG (v1)
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "backend"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

# Seed local LKG only.
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5

# Assert runtime serves local LKG (v1) before relay publish.
assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"

# Publish V2 to relay after runtime is up.
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
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
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" --data-binary "@$TEST_TMP/config_v2.pvs" > /dev/null

# Assert switch to relay current (v2).

MAX_RETRIES=20
SWITCHED=0
attempt=0
for attempt in $(seq 1 $MAX_RETRIES); do
    if pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo" | grep -q "backend-v2"; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done

assert_retry_succeeded "$attempt" "$MAX_RETRIES"

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Integrated recovery failed"
    exit 1
fi

echo "STEP: assert config version metric"
METRICS_OUT=$(pavis_curl_body "http://127.0.0.1:$PORT_METRICS" | tr -d '\r')
if ! echo "$METRICS_OUT" | grep -q 'pavis_runtime_config_version{version="1"}'; then
    echo "❌ Expected config version label not found"
    echo "$METRICS_OUT" | grep "pavis_runtime_config_version" || true
    exit 1
fi

echo "✅ 30_lkg_artifact passed"

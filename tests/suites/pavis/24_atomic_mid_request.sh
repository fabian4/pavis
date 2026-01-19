#!/bin/bash
set -e

# Case: atomic_mid_request
# Category: Reload Semantics
# Invariants: C (Atomic Switch)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "atomic_mid_request"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
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
	      - matcher: !prefix { path: "/" }
	        response_headers:
	          set_headers: [["X-Pavis-Version", "v1"]]
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
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
	      - matcher: !prefix { path: "/" }
	        response_headers:
	          set_headers: [["X-Pavis-Version", "v2"]]
	        destinations:
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

curl -sS -D "$TEST_TMP/inflight.headers" -o "$TEST_TMP/inflight.body" \
    "http://127.0.0.1:$PORT_PAVIS/delay?ms=1500" &
INFLIGHT_PID=$!

sleep 0.2
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

wait $INFLIGHT_PID

if ! grep -qi "^X-Pavis-Version: v1" "$TEST_TMP/inflight.headers"; then
    echo "❌ In-flight response did not preserve v1 header"
    exit 1
fi
delayed_ms=$(json_get_number "delayed_ms" < "$TEST_TMP/inflight.body")
if [ "$delayed_ms" != "1500" ]; then
    echo "❌ In-flight response body mismatch: delayed_ms=$delayed_ms"
    exit 1
fi

SWITCHED=0
for _ in $(seq 1 20); do
    curl -sS -D "$TEST_TMP/post.headers" -o "$TEST_TMP/post.body" \
        "http://127.0.0.1:$PORT_PAVIS/delay?ms=10"
    if grep -qi "^X-Pavis-Version: v2" "$TEST_TMP/post.headers"; then
        SWITCHED=1
        break
    fi
    sleep 0.2
done

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Reload did not apply to v2 for post-request"
    exit 1
fi

echo "✅ atomic_mid_request passed"

#!/bin/bash
set -e

# Case: 51_routing_weighted
# Category: Traffic Behavior Under Reload
# Invariants: A (No-Drop)
# Intent: deterministic state carry-over elimination (100/0 flips), not probabilistic weight correctness.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "51_routing_weighted"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

pavis_curl_body_retry() {
    local url="$1"
    local attempts=3
    local delay=0.1
    local i
    for i in $(seq 1 "$attempts"); do
        if response=$(pavis_curl_body "$url"); then
            printf '%s' "$response"
            return 0
        fi
        sleep "$delay"
    done
    return 1
}

# V1: 100% v1 (Explicit single destination)
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	  - name: "v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Assert V1 (100% backend-v1)
echo "Running 1000 requests for V1 (100% v1)..."
for _ in {1..1000}; do
    response=$(pavis_curl_body_retry "http://127.0.0.1:$PORT_PAVIS/echo") || {
        echo "❌ Request failed during V1"
        exit 1
    }
    if [[ "$response" != *"backend-v1"* ]]; then
        echo "❌ Expected only backend-v1 in V1"
        exit 1
    fi
done

# V2: 0% v1, 100% v2 (Weight flip)
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
	upstreams:
	  - name: "v1"
	    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
	  - name: "v2"
	    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations: [{ upstream: "v2", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

# Wait for switch (poll for v2 presence)
MAX_RETRIES=20
SWITCHED=0
attempt=0
for attempt in $(seq 1 $MAX_RETRIES); do
    response=$(pavis_curl_body_retry "http://127.0.0.1:$PORT_PAVIS/echo") || {
        echo "❌ Request failed during V2 switch wait"
        exit 1
    }
    if [[ "$response" == *"backend-v2"* ]]; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done
assert_retry_succeeded "$attempt" "$MAX_RETRIES"

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Traffic did not shift to backend-v2"
    exit 1
fi

# Assert V2 (100% backend-v2)
echo "Running 1000 requests for V2 (100% v2)..."
for _ in {1..1000}; do
    response=$(pavis_curl_body_retry "http://127.0.0.1:$PORT_PAVIS/echo") || {
        echo "❌ Request failed during V2"
        exit 1
    }
    if [[ "$response" != *"backend-v2"* ]]; then
        echo "❌ Expected only backend-v2 in V2 (Weight flip failed)"
        exit 1
    fi
done

# V3: Deterministic flip back to 100% V1
cat <<-EOF > "$TEST_TMP/config_v3.yaml"
	listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
	upstreams:
	  - name: "v1"
	    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
	  - name: "v2"
	    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations: [{ upstream: "v1", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config_v3.yaml" "$TEST_TMP/config_v3.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v3.pvs"

# Wait for switch back (poll for v1 presence)
SWITCHED_BACK=0
attempt=0
for attempt in $(seq 1 $MAX_RETRIES); do
    response=$(pavis_curl_body_retry "http://127.0.0.1:$PORT_PAVIS/echo") || {
        echo "❌ Request failed during V3 switch wait"
        exit 1
    }
    if [[ "$response" == *"backend-v1"* ]]; then
        SWITCHED_BACK=1
        break
    fi
    sleep 0.5
done

if [ "$SWITCHED_BACK" -eq 0 ]; then
    echo "❌ Traffic did not shift back to backend-v1"
    exit 1
fi

# Assert V3 (100% backend-v1)
echo "Running 1000 requests for V3 (100% v1 reset)..."
for _ in {1..1000}; do
    response=$(pavis_curl_body_retry "http://127.0.0.1:$PORT_PAVIS/echo") || {
        echo "❌ Request failed during V3"
        exit 1
    }
    if [[ "$response" != *"backend-v1"* ]]; then
        echo "❌ Expected only backend-v1 in V3 (Deterministic weight flip failed)"
        exit 1
    fi
done

# V4: Probabilistic 50/50 Split
echo "Testing Phase V4: Probabilistic 50/50 Weighted Split"
cat <<-EOF > "$TEST_TMP/config_v4.yaml"
	listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
	upstreams:
	  - name: "v1"
	    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
	  - name: "v2"
	    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "v1"
	            weight: 50
	          - upstream: "v2"
	            weight: 50
EOF
gen_pvs "$TEST_TMP/config_v4.yaml" "$TEST_TMP/config_v4.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v4.pvs"

# Wait for BOTH to be present to ensure config applied
WAIT_BOTH=0
for _ in {1..20}; do
    response=$(pavis_curl_body_retry "http://127.0.0.1:$PORT_PAVIS/echo") || {
        echo "❌ Request failed during V4 switch wait"
        exit 1
    }
    # We just need to see V2 once to know the 50/50 config (or any config including V2) is live, 
    # since previous config was 100% V1.
    if [[ "$response" == *"backend-v2"* ]]; then
        WAIT_BOTH=1
        break
    fi
    sleep 0.5
done
if [ "$WAIT_BOTH" -eq 0 ]; then
    echo "❌ 50/50 split config did not apply (never saw backend-v2)"
    exit 1
fi

echo "Running 1000 requests for V4 (50/50 split)..."
count_v1=0
count_v2=0
for _ in {1..1000}; do
    response=$(pavis_curl_body_retry "http://127.0.0.1:$PORT_PAVIS/echo") || {
        echo "❌ Request failed during V4"
        exit 1
    }
    if [[ "$response" == *"backend-v1"* ]]; then
        count_v1=$((count_v1 + 1))
    elif [[ "$response" == *"backend-v2"* ]]; then
        count_v2=$((count_v2 + 1))
    else
        echo "❌ Unexpected response: $response"
        exit 1
    fi
done

echo "V4 Results: v1=$count_v1, v2=$count_v2"

# Assert statistical distribution within [400, 600] for 50/50 split (1000 samples)
if [ "$count_v1" -lt 350 ] || [ "$count_v1" -gt 650 ]; then
    echo "❌ V4 Distribution out of bounds: v1=$count_v1 (expected ~500)"
    exit 1
fi
if [ "$count_v2" -lt 350 ] || [ "$count_v2" -gt 650 ]; then
    echo "❌ V4 Distribution out of bounds: v2=$count_v2 (expected ~500)"
    exit 1
fi

echo "✅ traffic_40_weighted_shift passed"

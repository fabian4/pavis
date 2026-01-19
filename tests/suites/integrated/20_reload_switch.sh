#!/bin/bash
set -e

# Case: reload_01_traffic_shift
# Category: End-to-End Reload
# Invariants: I1, I2, I5

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "reload_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

# 1. Start Relay
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

# 2. Config V1
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

# Publish V1
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/config_v1.pvs" > /dev/null

# Start Pavis (using V1 as initial LKG)
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Assert V1
response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
instance=$(echo "$response" | json_get_string "instance_id")
if [ "$instance" != "backend-v1" ]; then
    echo "❌ Expected backend-v1, got $instance"
    exit 1
fi

# Capture ID to ensure no restart
SUT_ID_INITIAL=$(get_sut_id "pavis")

# 3. Publish V2
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

BURST_COUNT=200
(
    for i in $(seq 1 $BURST_COUNT); do
        headers="$TEST_TMP/burst_$i.headers"
        body="$TEST_TMP/burst_$i.body"
        if ! curl -sS -D "$headers" -o "$body" "http://127.0.0.1:$PORT_PAVIS/echo"; then
            echo "FAIL" > "$TEST_TMP/burst_$i.fail"
        fi
    done
) &
TRAFFIC_PID=$!

sleep 0.1
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$TEST_TMP/config_v2.pvs" > /dev/null

wait $TRAFFIC_PID

V1_COUNT=0
V2_COUNT=0
FAIL_COUNT=0
V2_STARTED=0
for i in $(seq 1 $BURST_COUNT); do
    if [ -f "$TEST_TMP/burst_$i.fail" ]; then
        FAIL_COUNT=$((FAIL_COUNT+1))
        continue
    fi
    instance=$(json_get_string "instance_id" < "$TEST_TMP/burst_$i.body")
    if [ "$instance" = "backend-v2" ]; then
        V2_COUNT=$((V2_COUNT+1))
        V2_STARTED=1
    elif [ "$instance" = "backend-v1" ]; then
        V1_COUNT=$((V1_COUNT+1))
        if [ $V2_STARTED -eq 1 ]; then
            echo "❌ Non-atomic switch: V1 seen after V2 at request $i"
            exit 1
        fi
    else
        echo "❌ Unexpected instance_id during reload burst: $instance"
        FAIL_COUNT=$((FAIL_COUNT+1))
    fi
done

if [ $FAIL_COUNT -gt 0 ]; then
    echo "❌ Invariant violated: $FAIL_COUNT requests failed during reload"
    exit 1
fi

# 4. Wait for Switch
MAX_RETRIES=20
SWITCHED=0
for _ in $(seq 1 $MAX_RETRIES); do
    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
    instance=$(echo "$response" | json_get_string "instance_id")
    
    if [ "$instance" == "backend-v2" ]; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Traffic did not switch to backend-v2"
    exit 1
fi

# 5. Assert Identity
SUT_ID_FINAL=$(get_sut_id "pavis")
if [ "$SUT_ID_INITIAL" != "$SUT_ID_FINAL" ]; then
    echo "❌ SUT identity changed! Possible restart."
    exit 1
fi

echo "✅ reload_01_traffic_shift passed"

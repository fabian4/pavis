#!/bin/bash
set -e

# Case: reload_contract_core
# Category: Reload Semantics
# Invariants: A (No-Drop), C (Atomic Switch), D (Zero-Option)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "reload_contract_core"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# V1: backend-v1 + header present.
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
	      - matcher: !prefix
	          path: "/"
	        response_headers:
	          set_headers: [["X-Pavis-Version", "v1"]]
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

# V2: backend-v2 + header removed entirely.
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
	      - matcher: !prefix
	          path: "/"
	        destinations:
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"

cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"

wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
echo "$response" | assert_json_has_key "instance_id"
instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin).get('instance_id',''))")
if [ "$instance" != "backend-v1" ]; then
    echo "❌ Expected backend-v1 initially, got $instance"
    exit 1
fi

V1_HEADERS="$TEST_TMP/v1.headers"
pavis_curl_headers "$V1_HEADERS" "http://127.0.0.1:$PORT_PAVIS/echo"
if ! grep -qi "^X-Pavis-Version: v1" "$V1_HEADERS"; then
    echo "❌ Expected X-Pavis-Version: v1 header on V1"
    exit 1
fi

SUT_ID_INITIAL=$(get_sut_id "pavis")

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
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

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
    instance=$(python3 -c "import sys, json; print(json.load(sys.stdin).get('instance_id',''))" < "$TEST_TMP/burst_$i.body")
    if [ "$instance" = "backend-v2" ]; then
        V2_COUNT=$((V2_COUNT+1))
        V2_STARTED=1
        if grep -qi "^X-Pavis-Version:" "$TEST_TMP/burst_$i.headers"; then
            echo "❌ Header removed in V2, but still present during reload"
            exit 1
        fi
    elif [ "$instance" = "backend-v1" ]; then
        V1_COUNT=$((V1_COUNT+1))
        if [ $V2_STARTED -eq 1 ]; then
            echo "❌ Non-atomic switch: V1 seen after V2 at request $i"
            exit 1
        fi
    else
        echo "❌ Unexpected instance_id: $instance"
        FAIL_COUNT=$((FAIL_COUNT+1))
    fi
done

echo "Burst results: v1=$V1_COUNT, v2=$V2_COUNT, fail=$FAIL_COUNT"
if [ $FAIL_COUNT -gt 0 ]; then
    echo "❌ Invariant A violated: $FAIL_COUNT requests failed during reload"
    exit 1
fi

SWITCHED=0
for _ in $(seq 1 20); do
    headers="$TEST_TMP/switch.headers"
    body="$TEST_TMP/switch.body"
    if curl -sS -D "$headers" -o "$body" "http://127.0.0.1:$PORT_PAVIS/echo"; then
        instance=$(python3 -c "import sys, json; print(json.load(sys.stdin).get('instance_id',''))" < "$body")
        if [ "$instance" = "backend-v2" ] && ! grep -qi "^X-Pavis-Version:" "$headers"; then
            SWITCHED=1
            break
        fi
    fi
    sleep 0.5
done

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Reload did not converge to V2 with header removal"
    exit 1
fi

POST_COUNT=50
for i in $(seq 1 $POST_COUNT); do
    headers="$TEST_TMP/post_$i.headers"
    body="$TEST_TMP/post_$i.body"
    curl -sS -D "$headers" -o "$body" "http://127.0.0.1:$PORT_PAVIS/echo"
    instance=$(python3 -c "import sys, json; print(json.load(sys.stdin).get('instance_id',''))" < "$body")
    if [ "$instance" != "backend-v2" ]; then
        echo "❌ Post-switch request saw $instance"
        exit 1
    fi
    if grep -qi "^X-Pavis-Version:" "$headers"; then
        echo "❌ Removed header still present after switch"
        exit 1
    fi
done

SUT_ID_FINAL=$(get_sut_id "pavis")
if [ "$SUT_ID_INITIAL" != "$SUT_ID_FINAL" ]; then
    echo "❌ SUT identity changed! Possible restart."
    exit 1
fi

if ! check_sut_alive "pavis"; then
    echo "❌ Pavis is not running!"
    exit 1
fi

echo "✅ reload_contract_core passed"

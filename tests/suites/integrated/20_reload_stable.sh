#!/bin/bash
set -e

# Case: 20_reload_stable
# Category: End-to-End Reload
# Invariants: I2 (Hot Reload Pipeline), A (No-Drop)
#
# This test verifies that traffic is NOT interrupted and NO requests are dropped
# during an idempotent configuration update (same artifact, new version).

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "20_reload_stable"
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
EOF
run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 2. Generate Config V1
cat <<-EOF > "$TEST_TMP/config.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
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
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# Publish V1 (ver 1)
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/config.pvs" > /dev/null

# Start Pavis
cp "$TEST_TMP/config.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Verify initial serving
assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"

# 3. Burst Traffic during Idempotent Update
echo "Starting traffic burst during idempotent update..."
BURST_COUNT=200
TRAFFIC_STARTED="$TEST_TMP/traffic_started"
(
    for i in $(seq 1 $BURST_COUNT); do
        headers="$TEST_TMP/burst_$i.headers"
        body="$TEST_TMP/burst_$i.body"
        if ! curl -sS -D "$headers" -o "$body" "http://127.0.0.1:$PORT_PAVIS/echo"; then
            echo "FAIL" > "$TEST_TMP/burst_$i.fail"
        fi
        if [ "$i" -eq 1 ]; then
            touch "$TRAFFIC_STARTED"
        fi
        sleep 0.02
    done
) &
TRAFFIC_PID=$!

# Wait until traffic starts so reload overlaps with the burst.
for _ in $(seq 1 50); do
    [ -f "$TRAFFIC_STARTED" ] && break
    sleep 0.05
done

# Publish SAME V1 again (ver 2)
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/config.pvs" > /dev/null

wait $TRAFFIC_PID

# 4. Assert Zero-Drop and Continuity
FAIL_COUNT=0
BAD_CONTENT_COUNT=0
for i in $(seq 1 $BURST_COUNT); do
    if [ -f "$TEST_TMP/burst_$i.fail" ]; then
        FAIL_COUNT=$((FAIL_COUNT+1))
        continue
    fi
    
    # Check status code 200
    if ! grep -q "200 OK" "$TEST_TMP/burst_$i.headers"; then
        FAIL_COUNT=$((FAIL_COUNT+1))
    fi
    
    # Check content is still backend-v1
    if ! grep -q "backend-v1" "$TEST_TMP/burst_$i.body"; then
        BAD_CONTENT_COUNT=$((BAD_CONTENT_COUNT+1))
    fi
done

echo "Traffic results: failures=$FAIL_COUNT, bad_content=$BAD_CONTENT_COUNT"

if [ "$FAIL_COUNT" -gt 0 ]; then
    fail "Traffic interrupted during idempotent update: $FAIL_COUNT requests failed"
fi

if [ "$BAD_CONTENT_COUNT" -gt 0 ]; then
    fail "Traffic saw unexpected content during idempotent update: $BAD_CONTENT_COUNT requests"
fi

APPLY_COUNT=$(grep -c 'event="config_apply" result="ok"' "$TEST_TMP/logs/pavis.log" || true)
if [ "${APPLY_COUNT:-0}" -gt 0 ]; then
    fail "Reload applied on identical publish (config_apply count: $APPLY_COUNT)"
fi

echo "✅ 20_reload_stable passed"

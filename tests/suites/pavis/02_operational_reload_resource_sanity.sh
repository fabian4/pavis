#!/bin/bash
set -e

# Case: 02_operational_reload_resource_sanity
# Category: Operational Lifecycle
# Invariants: Resource Stability (FDs, RSS)
#
# This test verifies that rapid reloads do not cause monotonic resource leaks.
# Thresholds:
# 1. Monotonic Increase: FDs and RSS should not increase every single time a reload occurs.
#    Pingora might temporarily hold resources, but it should settle or fluctuate.
# 2. Absolute Ceiling: Total RSS increase across the storm should be within reasonable bounds
#    (e.g., < 100% increase from baseline for small configurations).

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "02_operational_reload_resource_sanity"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

write_config() {
    local version="$1"
    cat <<-EOF
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
	        response_headers:
	          set_headers: [["X-Pavis-Version", "v${version}"]]
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
}

write_config 1 > "$TEST_TMP/config_v1.yaml"
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

PID=$(get_sut_host_pid "pavis")
RESOURCE_OK=0
# Check if we can sample resources (Darwin has different ps flags, Linux uses /proc)
if [ -n "$PID" ]; then
    if [ -d "/proc/$PID" ]; then
        RESOURCE_OK=1
    elif ps -p "$PID" -o rss= >/dev/null 2>&1; then
        RESOURCE_OK=2
    fi
fi

sample_fds() {
    if [ "$RESOURCE_OK" -eq 1 ]; then
        ls "/proc/$PID/fd" 2>/dev/null | wc -l | tr -d ' '
    elif [ "$(uname)" = "Darwin" ]; then
        lsof -p "$PID" 2>/dev/null | wc -l | tr -d ' '
    else
        echo 0
    fi
}

sample_rss_kb() {
    if [ "$RESOURCE_OK" -eq 1 ]; then
        awk '/VmRSS:/ {print $2}' "/proc/$PID/status" 2>/dev/null
    else
        ps -o rss= -p "$PID" | tr -d ' '
    fi
}

collect_fds=()
collect_rss=()

# Initial Sample
RSS_BASELINE=$(sample_rss_kb)
echo "Baseline RSS: ${RSS_BASELINE} KB"

RELOAD_COUNT=6
for v in $(seq 2 $((RELOAD_COUNT+1))); do
    write_config "$v" > "$TEST_TMP/config_v${v}.yaml"
    gen_pvs "$TEST_TMP/config_v${v}.yaml" "$TEST_TMP/config_v${v}.pvs"
    publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v${v}.pvs"

    attempt=0
    for attempt in $(seq 1 20); do
        headers=$(curl -sI "http://127.0.0.1:$PORT_PAVIS/echo")
        if echo "$headers" | grep -qi "^X-Pavis-Version: v${v}"; then
            break
        fi
        sleep 0.2
    done
    assert_retry_succeeded "$attempt" 20
    
    if [ "$RESOURCE_OK" -gt 0 ]; then
        collect_fds+=("$(sample_fds)")
        collect_rss+=("$(sample_rss_kb)")
    fi
done

if [ "$RESOURCE_OK" -eq 0 ]; then
    echo "INFO: Resource sampling unavailable for PID '$PID'"
    echo "✅ operational_reload_resource_sanity passed (skipped sampling)"
    exit 0
fi

# Monotonic increase check (Strictly Greater)
# If a value increases EVERY time, it's almost certainly a leak.
is_monotonically_increasing() {
    local -n values=$1
    local i
    local increases=0
    for ((i=1; i<${#values[@]}; i++)); do
        if [ "${values[$i]}" -gt "${values[$((i-1))]}" ]; then
            increases=$((increases+1))
        fi
    done
    # If it increased every single time
    if [ "$increases" -eq "$((${#values[@]} - 1))" ]; then
        return 0
    fi
    return 1
}

if is_monotonically_increasing collect_fds; then
    fail "FD count leaked monotonically across $RELOAD_COUNT reloads: ${collect_fds[*]}"
fi

if is_monotonically_increasing collect_rss; then
    fail "RSS leaked monotonically across $RELOAD_COUNT reloads: ${collect_rss[*]} KB"
fi

# Absolute Ceiling Check
# Rationale: Small configs can cause transient allocator growth (TLS stacks,
# connection pools, metrics buffers). We allow up to 2x baseline to avoid
# false positives while still catching monotonic leaks.
RSS_FINAL=$(sample_rss_kb)
echo "Final RSS: ${RSS_FINAL} KB"
RSS_MAX_ALLOWED=$((RSS_BASELINE * 2)) # Allow doubling for safety, but not more
if [ "$RSS_FINAL" -gt "$RSS_MAX_ALLOWED" ]; then
    fail "RSS exceeded absolute ceiling: ${RSS_FINAL} KB (Baseline: ${RSS_BASELINE} KB)"
fi

echo "✅ operational_reload_resource_sanity passed"

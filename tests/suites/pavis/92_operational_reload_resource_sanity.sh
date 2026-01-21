#!/bin/bash
set -e

# Case: operational_reload_resource_sanity
# Category: Operational Lifecycle
# Invariants: (coarse resource sanity during reloads)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "operational_reload_resource_sanity"
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

PID=$(get_sut_id "pavis")
RESOURCE_OK=0
if [ -n "$PID" ] && [ -d "/proc/$PID" ]; then
    RESOURCE_OK=1
fi

sample_fds() {
    ls "/proc/$PID/fd" 2>/dev/null | wc -l | tr -d ' '
}

sample_rss_kb() {
    awk '/VmRSS:/ {print $2}' "/proc/$PID/status" 2>/dev/null
}

collect_versions=()
collect_fds=()
collect_rss=()

for v in $(seq 2 7); do
    write_config "$v" > "$TEST_TMP/config_v${v}.yaml"
    gen_pvs "$TEST_TMP/config_v${v}.yaml" "$TEST_TMP/config_v${v}.pvs"
    publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v${v}.pvs"

    SWITCHED=0
    for _ in $(seq 1 20); do
        headers=$(curl -sI "http://127.0.0.1:$PORT_PAVIS/echo")
        if echo "$headers" | grep -qi "^X-Pavis-Version: v${v}"; then
            SWITCHED=1
            break
        fi
        sleep 0.2
    done
    if [ "$SWITCHED" -eq 0 ]; then
        echo "❌ Reload did not apply version v${v}"
        exit 1
    fi

    collect_versions+=("$v")
    if [ "$RESOURCE_OK" -eq 1 ]; then
        collect_fds+=("$(sample_fds)")
        collect_rss+=("$(sample_rss_kb)")
    fi
done

if [ "$RESOURCE_OK" -ne 1 ]; then
    echo "INFO: resource sampling unavailable for PID '$PID'"
    echo "✅ operational_reload_resource_sanity passed"
    exit 0
fi

monotonic_increase() {
    local -n values=$1
    local i
    for ((i=1; i<${#values[@]}; i++)); do
        if [ "${values[$i]}" -le "${values[$((i-1))]}" ]; then
            return 1
        fi
    done
    return 0
}

if monotonic_increase collect_fds; then
    echo "❌ FD count increased monotonically across reloads: ${collect_fds[*]}"
    exit 1
fi

if monotonic_increase collect_rss; then
    echo "❌ RSS increased monotonically across reloads: ${collect_rss[*]} KB"
    exit 1
fi

echo "✅ operational_reload_resource_sanity passed"

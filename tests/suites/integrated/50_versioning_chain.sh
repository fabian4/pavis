#!/bin/bash
set -e

# Case: 50_versioning_chain
# Category: End-to-End Reload
# Invariants: I2 (Monotonic), I5 (No Regression)

if [ "${E2E_VERBOSE:-0}" -eq 1 ]; then
    set -x
fi

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "50_versioning_chain"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

TEST_TIMEOUT="${TEST_TIMEOUT:-120}"
(
    sleep "$TEST_TIMEOUT"
    echo "❌ multiversion_50 timed out after ${TEST_TIMEOUT}s"
    kill -TERM "$$"
) &
WATCHDOG_PID=$!
trap 'kill "$WATCHDOG_PID" 2>/dev/null || true' EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_METRICS=$(get_free_port)
echo "Ports: pavis=$PORT_PAVIS relay=$PORT_RELAY metrics=$PORT_METRICS"

# 1. Start Relay
cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	artifact:
	  lkg_path: "$TEST_TMP/lkg.pvs"
EOF
echo "Starting relay"
run_relay "$TEST_TMP/relay.yaml"
echo "Waiting for relay health"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5 --connect-timeout 1 --max-time 2
echo "Relay is healthy"

# V1
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
        response_headers:
          set_headers: [["X-Backend-Version", "v1"]]
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

# V2
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
        response_headers:
          set_headers: [["X-Backend-Version", "v2"]]
	        destinations:
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

# V3
cat <<-EOF > "$TEST_TMP/config_v3.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "backend-v3"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
        response_headers:
          set_headers: [["X-Backend-Version", "v3"]]
	        destinations:
	          - upstream: "backend-v3"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v3.yaml" "$TEST_TMP/config_v3.pvs"

# V4
cat <<-EOF > "$TEST_TMP/config_v4.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "backend-v4"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
        response_headers:
          set_headers: [["X-Backend-Version", "v4"]]
	        destinations:
	          - upstream: "backend-v4"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v4.yaml" "$TEST_TMP/config_v4.pvs"

# Publish V1 and start runtime
echo "Publishing v1"
curl -s -f --connect-timeout 2 --max-time 5 -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/config_v1.pvs" > /dev/null
echo "Published v1"

cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
echo "Starting pavis runtime"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
echo "Waiting for pavis healthz"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5 --connect-timeout 1 --max-time 2
echo "Waiting for metrics port"
wait_for_port "$PORT_METRICS" 5
echo "Metrics port is open"

METRICS_URL="http://127.0.0.1:$PORT_METRICS/metrics"
INITIAL_VERSION=$(wait_for_runtime_config_version "$METRICS_URL" "" 10 || true)
if [ "$INITIAL_VERSION" != "1" ]; then
    echo "❌ Expected initial runtime version 1, got '$INITIAL_VERSION'"
    exit 1
fi
echo "Initial runtime version is 1"

wait_for_version() {
    local expected="$1"
    local timeout="${2:-20}"
    local retries=$((timeout * 4))
    local backoff=0.25

    for _ in $(seq 1 $retries); do
        version=$(get_runtime_config_version "$METRICS_URL") || true
        if [ "$version" = "$expected" ]; then
            return 0
        fi
        sleep "$backoff"
    done
    return 1
}

publish_version() {
    local version="$1"
    local pvs_path="$2"
    curl -s -f --connect-timeout 2 --max-time 5 -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
        --data-binary "@$pvs_path" > /dev/null
}

start_version_monitor() {
    local out_file="$1"
    shift
    : > "$out_file"
    (
        local expected
        for expected in "$@"; do
            local retries=200
            while [ "$retries" -gt 0 ]; do
                if curl -s --connect-timeout 1 --max-time 2 "$METRICS_URL" | tr -d '\r' | \
                    grep -q "pavis_runtime_config_version{version=\"$expected\"}"; then
                    printf '%s\n' "$expected" >> "$out_file"
                    break
                fi
                retries=$((retries - 1))
                sleep 0.1
            done
        done
    ) >/dev/null 2>&1 &
    echo $!
}

assert_versions_in_order() {
    local out_file="$1"
    local expected_sequence="$2"
    local observed
    observed=$(awk '{
        if ($0 != last) {
            seq = seq $0 " "
            last = $0
        }
    } END { print seq }' "$out_file")
    for expected in $expected_sequence; do
        case " $observed " in
            *" $expected "*) observed="${observed#* $expected }" ;;
            *) echo "❌ Missing version $expected in monitor log"; return 1 ;;
        esac
    done
    return 0
}

wait_for_monitor_log() {
    local expected_version="$1"
    local log_file="$2"
    local timeout="${3:-10}"
    local retries=$((timeout * 10))

    for _ in $(seq 1 $retries); do
        if grep -q "^${expected_version}\$" "$log_file" 2>/dev/null; then
            return 0
        fi
        sleep 0.1
    done
    return 1
}

monitor_pid=$(start_version_monitor "$TEST_TMP/runtime_versions.log" 2 3 4)
trap 'kill "$monitor_pid" 2>/dev/null || true' EXIT

# Publish V2 -> V3 -> V4 serialized to ensure chain
echo "Publishing v2..v4"
publish_version 2 "$TEST_TMP/config_v2.pvs"
wait_for_version 2 10 || exit 1
wait_for_monitor_log 2 "$TEST_TMP/runtime_versions.log" 5 || echo "⚠️ Monitor slow to log v2"

publish_version 3 "$TEST_TMP/config_v3.pvs"
wait_for_version 3 10 || exit 1
wait_for_monitor_log 3 "$TEST_TMP/runtime_versions.log" 5 || echo "⚠️ Monitor slow to log v3"

publish_version 4 "$TEST_TMP/config_v4.pvs"
echo "Published v2..v4"

if ! wait_for_version 4 20; then
    echo "❌ Runtime did not apply version 4"
    exit 1
fi
echo "Runtime reached version 4"

if ! wait "$monitor_pid"; then
    echo "⚠️ Version monitor exited before capturing all updates" >&2
fi

if ! assert_versions_in_order "$TEST_TMP/runtime_versions.log" "2 3 4"; then
    echo "❌ Runtime did not apply versions in order (2 -> 3 -> 4)"
    exit 1
fi

if ! check_sut_alive "pavis"; then
    echo "❌ Pavis died during multi-version chain"
    exit 1
fi

echo "✅ multiversion_01_chain_apply passed"

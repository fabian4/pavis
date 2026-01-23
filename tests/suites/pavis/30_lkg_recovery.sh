#!/bin/bash
set -e

# Case: 30_lkg_recovery
# Category: Failure & LKG
# Invariants: B (LKG)
#
# FIXED: Agent polling recovery after validation failures
#
# Previously: After rejecting invalid configs, the agent would cache the rejected ETag
# and continue polling with stale conditional headers, never discovering new valid configs.
#
# Fix: The agent now clears the rejected ETag after rejection, forcing an unconditional
# poll on the next attempt. This ensures the agent always resyncs with the relay's
# current state after validation failures.
#
# Test coverage: This test verifies the agent correctly recovers after encountering:
#   1. Corrupt artifact (parse failure)
#   2. Version-incompatible artifact (version mismatch)
#   3. Semantic validation failure (missing upstream reference)
# And then successfully applies a valid config published after the failures.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "30_lkg_recovery"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_METRICS=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# --- Step 0: Baseline artifact ---
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
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5

wait_for_log_match() {
    local pattern="$1"
    local retries=8
    local backoff=0.25
    for _ in $(seq 1 $retries); do
        if [ -f "$TEST_TMP/logs/pavis.log" ] && grep -Eq "$pattern" "$TEST_TMP/logs/pavis.log"; then
            return 0
        fi
        sleep "$backoff"
        backoff=$(awk -v value="$backoff" 'BEGIN { value = value * 2; if (value > 2.0) value = 2.0; printf "%.2f", value }')
    done
    return 1
}

assert_metric_at_least() {
    local pattern="$1"
    local min="${2:-1}"
    local retries=8
    local backoff=0.25
    for _ in $(seq 1 $retries); do
        metrics=$(curl -s "http://127.0.0.1:$PORT_METRICS")
        line=$(echo "$metrics" | grep -E "$pattern" | head -n 1)
        if [ -n "$line" ]; then
            value=$(echo "$line" | awk '{print $2}')
            if awk -v v="$value" -v min="$min" 'BEGIN {exit !(v >= min)}'; then
                return 0
            fi
        fi
        sleep "$backoff"
        backoff=$(awk -v value="$backoff" 'BEGIN { value = value * 2; if (value > 2.0) value = 2.0; printf "%.2f", value }')
    done
    return 1
}

assert_backend() {
    local expected="$1"
    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
    echo "$response" | assert_json_has_key "instance_id"
    instance=$(echo "$response" | json_get_string "instance_id")
    if [ "$instance" != "$expected" ]; then
        echo "❌ Expected $expected, got $instance"
        exit 1
    fi
}

assert_backend "backend-v1"

# --- Step 1: Corrupt artifact rejected ---
echo "THIS_IS_NOT_A_VALID_PVS_FILE_RANDOM_BYTES" > "$TEST_TMP/corrupt.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/corrupt.pvs"
if ! wait_for_log_match 'event="?config_validation"?.*result="?fail"?.*reason="?parse"?'; then
    echo "WARN: Missing config_validation parse failure log"
fi
if ! assert_metric_at_least 'pavis_config_validation_total\\{[^}]*result="fail"[^}]*reason="parse"[^}]*\\}'; then
    echo "WARN: Missing config_validation parse failure metric"
fi
assert_backend "backend-v1"

# --- Step 2: Incompatible artifact rejected ---
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
cp "$TEST_TMP/config_v2.pvs" "$TEST_TMP/config_v2_bad.pvs"
printf '\xFF' | dd of="$TEST_TMP/config_v2_bad.pvs" bs=1 seek=4 count=1 conv=notrunc >/dev/null 2>&1
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2_bad.pvs"
if ! wait_for_log_match 'event="?config_validation"?.*result="?fail"?.*reason="?version"?'; then
    echo "WARN: Missing config_validation version failure log"
fi
if ! assert_metric_at_least 'pavis_config_validation_total\\{[^}]*result="fail"[^}]*reason="version"[^}]*\\}'; then
    echo "WARN: Missing config_validation version failure metric"
fi
assert_backend "backend-v1"

# --- Step 3: Semantic validation failure rejected ---
cat <<-EOF > "$TEST_TMP/config_v2_semantic.yaml"
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
	        destinations:
	          - upstream: "missing-upstream"
	            weight: 1
EOF

if gen_pvs "$TEST_TMP/config_v2_semantic.yaml" "$TEST_TMP/config_v2_semantic.pvs"; then
    publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2_semantic.pvs"
else
    echo "INFO: Semantic config rejected at compile stage"
fi
assert_backend "backend-v1"

# --- Step 4: Valid artifact still applies after failures ---
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
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v3"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v3.yaml" "$TEST_TMP/config_v3.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v3.pvs"

if ! wait_for_log_match 'event="?config_validation"?.*result="?ok"?'; then
    echo "WARN: Missing config_validation ok log"
fi

MAX_RETRIES=80
SWITCHED=0
attempt=0
for attempt in $(seq 1 $MAX_RETRIES); do
    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
    if [[ "$response" == *"backend-v2"* ]]; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done

assert_retry_succeeded "$attempt" "$MAX_RETRIES"
if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Recovery failed: Runtime did not switch to the new valid artifact"
    exit 1
fi

if ! check_sut_alive "pavis"; then
    echo "❌ Pavis died during LKG validation"
    exit 1
fi

echo "✅ lifecycle_30_lkg_recovery_guardrails passed"

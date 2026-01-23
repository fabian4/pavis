#!/bin/bash
# wait_helpers.sh
# Deterministic wait helpers for E2E tests
# Replaces sleep-based waits with bounded polling

# NOTE: wait_for_url is already defined in assert.sh - do not redefine here

# Wait for log pattern to appear
# Usage: wait_for_log "pattern" "/path/to/log.log" 10
wait_for_log() {
    local pattern="$1"
    local logfile="$2"
    local timeout="${3:-10}"
    local start_time
    start_time=$(date +%s)
    local end_time=$((start_time + timeout))

    while [ "$(date +%s)" -lt "$end_time" ]; do
        # Use -a to treat file as text (avoid binary file matches issue)
        if [ -f "$logfile" ] && grep -aEq "$pattern" "$logfile"; then
            return 0
        fi
        sleep 0.1
    done

    echo "❌ Timeout waiting for log pattern: $pattern" >&2
    return 1
}

# Wait for metric to match condition
# Usage: wait_for_metric "pavis_http_requests_total" "> 0" 10
wait_for_metric() {
    local metric="$1"
    local condition="$2"
    local timeout="${3:-10}"
    local metrics_url="${METRICS_URL:-http://127.0.0.1:9090/metrics}"
    local start_time
    start_time=$(date +%s)
    local end_time=$((start_time + timeout))

    while [ "$(date +%s)" -lt "$end_time" ]; do
        local value
        value=$(curl -s "$metrics_url" | grep -E "^${metric}" | head -n 1 | awk '{print $2}')
        # Use shell injection for the condition operator since awk doesn't support eval of string vars
        if [ -n "$value" ] && awk -v v="$value" "BEGIN {exit !(v $condition)}"; then
            return 0
        fi
        sleep 0.1
    done

    echo "❌ Timeout waiting for metric $metric $condition" >&2
    return 1
}

# Wait for config version to reach expected value
# Usage: wait_for_version "3" 10
wait_for_version() {
    local expected="$1"
    local timeout="${2:-10}"
    local admin_url="${ADMIN_URL:-http://127.0.0.1:6188/stats}"
    local start_time
    start_time=$(date +%s)
    local end_time=$((start_time + timeout))

    while [ "$(date +%s)" -lt "$end_time" ]; do
        local current
        current=$(curl -s "$admin_url" | jq -r '.config.version // empty' 2>/dev/null)
        if [ "$current" = "$expected" ]; then
            return 0
        fi
        sleep 0.1
    done

    echo "❌ Timeout waiting for version $expected" >&2
    return 1
}

# Wait for port to be listening
# Usage: wait_for_port 8080 10
wait_for_port() {
    local port="$1"
    local timeout="${2:-10}"
    local start_time
    start_time=$(date +%s)
    local end_time=$((start_time + timeout))

    while [ "$(date +%s)" -lt "$end_time" ]; do
        if nc -z 127.0.0.1 "$port" 2>/dev/null; then
            return 0
        fi
        sleep 0.1
    done

    echo "❌ Timeout waiting for port: $port" >&2
    return 1
}

# Assert that a retry loop succeeded within bounds
# Usage: assert_retry_succeeded $attempt $max_retries
assert_retry_succeeded() {
    local attempt=$1
    local max_retries=$2
    if [ "$attempt" -ge "$max_retries" ]; then
        echo "❌ Retry timeout: reached max attempts ($max_retries)" >&2
        exit 1
    fi
}

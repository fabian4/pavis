#!/bin/bash

# tests/lib/assert.sh
# Assertions and Wait helpers.

assert_body() {
    local url="$1"
    local expected="$2"
    local actual=$(curl -s "$url")
    if [[ "$actual" != *"$expected"* ]]; then
        echo "❌ Assertion failed: Expected body to contain '$expected', got '$actual'"
        return 1
    fi
}

assert_status() {
    local url="$1"
    local expected="$2"
    local actual=$(curl -s -o /dev/null -w "%{http_code}" "$url")
    if [ "$actual" != "$expected" ]; then
        echo "❌ Assertion failed: Expected status $expected, got $actual"
        return 1
    fi
}

assert_json_has_key() {
    local key="$1"
    # Read JSON from stdin
    if ! python3 -c "import sys, json; data=json.load(sys.stdin); assert '$key' in data, 'Key $key missing'" 2>/dev/null; then
        echo "❌ JSON assertion failed: Key '$key' missing in response"
        return 1
    fi
}

wait_for_url() {
    local url="$1"
    local timeout="${2:-30}"
    shift 2
    local extra_args="$@"
    local start_time=$(date +%s)

    while true; do
        if curl -s -o /dev/null $extra_args "$url"; then
            return 0
        fi
        local current_time=$(date +%s)
        if [ $((current_time - start_time)) -ge "$timeout" ]; then
            echo "Timeout waiting for $url"
            return 1
        fi
        sleep 0.5
    done
}

wait_for_port() {
    local port="$1"
    local timeout="${2:-10}"
    local start_time=$(date +%s)

    while true; do
        if nc -z 127.0.0.1 "$port" 2>/dev/null; then
            return 0
        fi
        local current_time=$(date +%s)
        if [ $((current_time - start_time)) -ge "$timeout" ]; then
            return 1
        fi
        sleep 0.2
    done
}
#!/bin/bash

# tests/scripts/assert.sh
# Assertions and Wait helpers.

assert_body() {
    local url="$1"
    local expected="$2"
    shift 2
    local actual
    actual=$(pavis_curl_body "$url" "$@")
    if [[ "$actual" != *"$expected"* ]]; then
        echo "❌ Assertion failed: Expected body to contain '$expected', got '$actual'"
        return 1
    fi
}

assert_status() {
    local url="$1"
    local expected="$2"
    shift 2
    local actual
    actual=$(pavis_curl_body -o /dev/null -w "%{http_code}" "$url" "$@")
    if [ "$actual" != "$expected" ]; then
        echo "❌ Assertion failed: Expected status $expected, got $actual"
        return 1
    fi
}

header_value() {
    local file="$1"
    local header_name="$2"
    # Case-insensitive grep for the header, using -a to treat as text even if binary body exists
    # We use sed to extract the value after the first colon and space, and trim \r
    grep -ai "^$header_name:" "$file" | head -n 1 | sed -E "s/^[^:]+:[[:space:]]*//i" | tr -d '\r' | tr -d '\n' | tr -d ' '
}

assert_status_eq() {
    local file="$1"
    local expected="$2"
    # First line of curl -i is "HTTP/1.1 200 OK"
    local actual
    actual=$(head -n 1 "$file" | awk '{print $2}')
    if [ "$actual" != "$expected" ]; then
        echo "❌ Assertion failed: Expected status $expected, got $actual"
        return 1
    fi
}

assert_header_eq() {
    local file="$1"
    local name="$2"
    local expected="$3"
    local actual
    actual=$(header_value "$file" "$name")
    if [ "$actual" != "$expected" ]; then
        echo "❌ Assertion failed: Expected header '$name' to be '$expected', got '$actual'"
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
    local extra_args=("$@")
    local start_time
    start_time=$(date +%s)

    while true; do
        if curl -s -o /dev/null "${extra_args[@]}" "$url"; then
            return 0
        fi
        local current_time
        current_time=$(date +%s)
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
    local start_time
    start_time=$(date +%s)

    while true; do
        if nc -z 127.0.0.1 "$port" 2>/dev/null; then
            return 0
        fi
        local current_time
        current_time=$(date +%s)
        if [ $((current_time - start_time)) -ge "$timeout" ]; then
            return 1
        fi
        sleep 0.2
    done
}

assert_eq() {
    local expected="$1"
    local actual="$2"
    local msg="$3"
    if [ "$actual" != "$expected" ]; then
        echo "❌ Assertion failed: $msg"
        echo "   Expected: '$expected'"
        echo "   Actual:   '$actual'"
        exit 1
    fi
}

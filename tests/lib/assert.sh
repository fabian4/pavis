#!/bin/bash

# tests/lib/assert.sh

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

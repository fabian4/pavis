#!/bin/bash

# e2e/scripts/lib/http.sh

wait_for_url() {
    local url="$1"
    local timeout="${2:-30}"
    local start_time=$(date +%s)

    # echo "Waiting for $url..."
    while true; do
        if curl -s -o /dev/null "$url"; then
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

assert_body() {
    local url="$1"
    local expected="$2"
    local resp=$(curl -s "$url")
    if [[ "$resp" != "$expected" ]]; then
        echo "❌ Expected '$expected', got '$resp'"
        return 1
    fi
    return 0
}

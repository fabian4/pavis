#!/bin/bash

# tests/lib/network.sh

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

get_free_port() {
    python3 -c 'import socket; s=socket.socket(); s.bind(("", 0)); print(s.getsockname()[1]); s.close()'
}

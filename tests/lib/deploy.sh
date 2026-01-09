#!/bin/bash

# tests/lib/deploy.sh

if [ -z "$PROJECT_ROOT" ]; then
    export PROJECT_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
fi
export TEST_MODE=${TEST_MODE:-binary}
export PAVIS_BIN=${PAVIS_BIN:-$PROJECT_ROOT/target/release/pavis}
export RELAY_BIN=${RELAY_BIN:-$PROJECT_ROOT/target/release/pavis-relay}
export PAVCTL_BIN=${PAVCTL_BIN:-$PROJECT_ROOT/target/release/pavctl}
export PAVIS_IMAGE=${PAVIS_IMAGE:-pavis:local}
export RELAY_IMAGE=${RELAY_IMAGE:-pavis-relay:local}

# Helper to get the address of the host from within a container
# On Linux with --network host, localhost works.
# On Mac/Windows, we need host.docker.internal.
get_host_addr() {
    if [ "$TEST_MODE" == "docker" ] && [[ "$OSTYPE" == "darwin"* ]]; then
        echo "host.docker.internal"
    else
        echo "127.0.0.1"
    fi
}

run_pavis() {
    local config_path="$1"
    local relay_url="$2"
    local name="${3:-pavis}"
    
    local args=("--config" "$config_path")
    if [ -n "$relay_url" ]; then
        args+=("--relay-url" "$relay_url")
    fi
    
    if [ "$TEST_MODE" == "binary" ]; then
        RUST_LOG=debug "$PAVIS_BIN" "${args[@]}" > "$TEST_TMP/logs/${name}.log" 2>&1 &
        record_pid $! "$name"
    else
        local docker_args=(
            run -d --rm
            --user "$(id -u):$(id -g)"
            --network host
            -v "$TEST_TMP:$TEST_TMP:ro"
        )
        local cmd_args=("--config" "$config_path")
        if [ -n "$relay_url" ]; then
            cmd_args+=("--relay-url" "$relay_url")
        fi

        local container_id=$(docker "${docker_args[@]}" "$PAVIS_IMAGE" "${cmd_args[@]}")
        record_container "$container_id" "$name"
    fi
}

run_relay() {
    local config_path="$1"
    local name="${2:-relay}"
    
    if [ "$TEST_MODE" == "binary" ]; then
        RUST_LOG=debug "$RELAY_BIN" --config "$config_path" > "$TEST_TMP/logs/${name}.log" 2>&1 &
        record_pid $! "$name"
    else
        local container_id=$(docker run -d --rm \
            --user "$(id -u):$(id -g)" \
            --network host \
            -v "$TEST_TMP:$TEST_TMP:rw" \
            "$RELAY_IMAGE" \
            --config "$config_path")
        record_container "$container_id" "$name"
    fi
}

gen_pvs() {
    local yaml_path="$1"
    local pvs_path="$2"
    "$PAVCTL_BIN" gen "$yaml_path" "$pvs_path"
}

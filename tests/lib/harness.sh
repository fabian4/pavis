#!/bin/bash

# tests/lib/harness.sh

setup_test() {
    local case_name="$1"
    local timestamp=$(date +%s%N)
    TEST_TMP="$PROJECT_ROOT/tests/temp/${case_name}_${timestamp}"
    mkdir -p "$TEST_TMP"
    export TEST_TMP
    echo "Using temp dir: $TEST_TMP"
    mkdir -p "$TEST_TMP/pids"
    mkdir -p "$TEST_TMP/logs"
    mkdir -p "$TEST_TMP/config"
}

record_pid() {
    local pid="$1"
    local name="$2"
    echo "$pid" > "$TEST_TMP/pids/$name.pid"
}

record_container() {
    local container_id="$1"
    local name="$2"
    echo "$container_id" > "$TEST_TMP/pids/$name.container"
}

cleanup_test() {
    echo "Cleaning up case environment..."
    
    if [ -d "$TEST_TMP/pids" ]; then
        for pid_file in "$TEST_TMP/pids"/*.pid; do
            [ -e "$pid_file" ] || continue
            local pid=$(cat "$pid_file")
            if kill -0 "$pid" 2>/dev/null; then
                kill -TERM "$pid" 2>/dev/null || kill -KILL "$pid" 2>/dev/null
            fi
        done
        
        for container_file in "$TEST_TMP/pids"/*.container; do
            [ -e "$container_file" ] || continue
            local container_id=$(cat "$container_file")
            docker stop "$container_id" >/dev/null 2>&1 || true
        done
    fi

    if [ "${KEEP_TMP:-false}" != "true" ]; then
        rm -rf "$TEST_TMP"
    else
        echo "Keeping temp dir: $TEST_TMP"
    fi
}

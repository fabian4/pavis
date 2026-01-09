#!/bin/bash

# tests/lib/env.sh
# Handles environment preparation, SUT lifecycle, and cleanup.

# Ensure Project Root is set
if [ -z "$PROJECT_ROOT" ]; then
    export PROJECT_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
fi

export TEST_MODE=${TEST_MODE:-binary}
export PAVIS_BIN=${PAVIS_BIN:-$PROJECT_ROOT/target/release/pavis}
export RELAY_BIN=${RELAY_BIN:-$PROJECT_ROOT/target/release/pavis-relay}
export PAVCTL_BIN=${PAVCTL_BIN:-$PROJECT_ROOT/target/release/pavctl}
export PAVIS_UPSTREAM_BIN=${PAVIS_UPSTREAM_BIN:-$PROJECT_ROOT/target/release/pavis-mock-upstream}
export MOCK_RELAY_BIN=${MOCK_RELAY_BIN:-$PROJECT_ROOT/target/release/pavis-mock-relay}
export PAVIS_IMAGE=${PAVIS_IMAGE:-pavis:local}
export RELAY_IMAGE=${RELAY_IMAGE:-pavis-relay:local}
export MOCK_RELAY_IMAGE=${MOCK_RELAY_IMAGE:-pavis-testkit:local}

CERTS_DIR="$PROJECT_ROOT/tests/config/certs"

setup_test() {
    local case_name="$1"
    local timestamp=$(date +%s%N)
    TEST_TMP="$PROJECT_ROOT/tests/temp/${case_name}_${timestamp}"
    mkdir -p "$TEST_TMP"
    export TEST_TMP
    
    if [ "${E2E_VERBOSE:-0}" -eq 1 ]; then
        echo "Using temp dir: $TEST_TMP"
    fi
    
    mkdir -p "$TEST_TMP/pids"
    mkdir -p "$TEST_TMP/logs"
    mkdir -p "$TEST_TMP/config"
}

cleanup_test() {
    local exit_code=$?
    
    # Kill processes
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

    if [ "$exit_code" -ne 0 ]; then
        echo "❌ Test FAILED (Exit: $exit_code)"
        echo "📂 Artifacts preserved at: $TEST_TMP"
        
        if [ -d "$TEST_TMP/logs" ]; then
            echo "--- LOG DUMP START ---"
            grep -r . "$TEST_TMP/logs" || echo "(No logs found)"
            echo "--- LOG DUMP END ---"
        fi
    elif [ "${KEEP_TMP:-false}" != "true" ]; then
        rm -rf "$TEST_TMP"
        if [ "${E2E_VERBOSE:-0}" -eq 1 ]; then
             echo "Cleaned up $TEST_TMP"
        fi
    else
        echo "Keeping temp dir: $TEST_TMP"
    fi
}

get_free_port() {
    python3 -c 'import socket; s=socket.socket(); s.bind(("", 0)); print(s.getsockname()[1]); s.close()'
}

generate_certs() {
    if [ "${E2E_VERBOSE:-0}" -eq 1 ]; then
        echo "🔑 Generating upstream certificates..."
    fi
    mkdir -p "$CERTS_DIR"
    openssl req -x509 -newkey rsa:2048 -nodes \
        -keyout "$CERTS_DIR/upstream_tls.key" \
        -out "$CERTS_DIR/upstream_tls.pem" \
        -subj "/CN=localhost" -days 365 2>/dev/null
}

cleanup_certs() {
    if [ "${E2E_VERBOSE:-0}" -eq 1 ]; then
        echo "🧹 Cleaning up upstream certificates..."
    fi
    rm -rf "$CERTS_DIR"
}

# SUT Management

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
            -e RUST_LOG=debug
            -v "$TEST_TMP:$TEST_TMP:rw"
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
    
    # Ensure the config file has valid storage paths to avoid errors on publish.
    # We use TEST_TMP as the root_dir and lkg.pvs as the filename.
    if ! grep -q "root_dir" "$config_path"; then
        if grep -q "storage:" "$config_path"; then
             sed -i.bak "/storage:/a\\
  root_dir: \"$TEST_TMP\"" "$config_path"
        else
             cat <<-EOF >> "$config_path"
storage:
  root_dir: "$TEST_TMP"
EOF
        fi
    fi
    if ! grep -q "lkg_path" "$config_path"; then
        if grep -q "artifact:" "$config_path"; then
             sed -i.bak "/artifact:/a\\
  lkg_path: \"lkg.pvs\"" "$config_path"
        else
             cat <<-EOF >> "$config_path"
artifact:
  lkg_path: "lkg.pvs"
EOF
        fi
    fi

    if [ "$TEST_MODE" == "binary" ]; then
        RUST_LOG=debug "$RELAY_BIN" --config "$config_path" > "$TEST_TMP/logs/${name}.log" 2>&1 &
        record_pid $! "$name"
    else
        local container_id=$(docker run -d --rm \
            --user "$(id -u):$(id -g)" \
            --network host \
            -e RUST_LOG=debug \
            -v "$TEST_TMP:$TEST_TMP:rw" \
            "$RELAY_IMAGE" \
            --config "$config_path")
        record_container "$container_id" "$name"
    fi
}

run_mock_relay() {
    local port="$1"
    local name="${2:-mock-relay}"

    if [ "$TEST_MODE" == "binary" ]; then
        RUST_LOG=debug "$MOCK_RELAY_BIN" --listen "127.0.0.1:$port" > "$TEST_TMP/logs/${name}.log" 2>&1 &
        record_pid $! "$name"
    else
        local container_id=$(docker run -d --rm \
            --user "$(id -u):$(id -g)" \
            --network host \
            -e RUST_LOG=debug \
            "$MOCK_RELAY_IMAGE" \
            /usr/local/bin/pavis-mock-relay --listen "0.0.0.0:$port")
        record_container "$container_id" "$name"
    fi
}

publish_config() {
    local relay_url="$1"
    local pvs_path="$2"
    
    curl -f -X POST "${relay_url}/publish" --data-binary "@${pvs_path}"
}

gen_pvs() {
    local yaml_path="$1"
    local pvs_path="$2"
    "$PAVCTL_BIN" gen "$yaml_path" "$pvs_path"
}

gen_minimal_pvs() {
    local pvs_path="$1"
    local id="${2:-default}"
    
    local yaml_path="${pvs_path}.yaml"
    cat <<-EOF > "$yaml_path"
	listeners: []
	upstreams:
	  - name: "dummy-$id"
	    endpoints: []
	routes: []
	telemetry:
	  service_name: "relay-test-$id"
EOF
    "$PAVCTL_BIN" gen "$yaml_path" "$pvs_path"
}

get_host_addr() {
    if [ "$TEST_MODE" == "docker" ] && [[ "$OSTYPE" == "darwin"* ]]; then
        echo "host.docker.internal"
    else
        echo "127.0.0.1"
    fi
}

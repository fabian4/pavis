#!/bin/bash

# tests/scripts/docker.sh
# Manages shared upstream infrastructure (either Docker Compose or local binaries).

if [ -z "$PROJECT_ROOT" ]; then
    PROJECT_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
    export PROJECT_ROOT
fi

SUITES_DIR="$PROJECT_ROOT/tests/suites"
UPSTREAMS_PID_DIR="$PROJECT_ROOT/tests/temp/upstreams"
UPSTREAMS_LOG_DIR="$PROJECT_ROOT/tests/temp/upstream-logs"
mkdir -p "$PROJECT_ROOT/tests/temp"

# Assumes env.sh is sourced for generate_certs/cleanup_certs and wait_for_port helper

can_bind_port() {
    local port
    port=$(get_free_port) || return 1
    [ -n "$port" ]
}

start_upstreams() {
    local suite="$1"
    if [ "$TEST_MODE" == "binary" ]; then
        start_upstreams_binary
    else
        start_upstreams_docker "$suite"
    fi
}

ensure_upstreams() {
    local suite="$1"
    if [ "$TEST_MODE" == "binary" ]; then
        ensure_upstreams_binary
    else
        ensure_upstreams_docker "$suite"
    fi
}

stop_upstreams() {
    local suite="$1"
    if [ "$TEST_MODE" == "binary" ]; then
        stop_upstreams_binary
    else
        stop_upstreams_docker "$suite"
    fi
    cleanup_certs
}

resolve_compose_file() {
    local suite="$1"
    if [ -n "$suite" ]; then
        local suite_compose="$SUITES_DIR/$suite/docker-compose.yaml"
        if [ -f "$suite_compose" ]; then
            echo "$suite_compose"
            return
        fi
    fi
    echo ""
}

start_upstreams_docker() {
    local suite="$1"
    local quiet="${2:-0}"
    local compose_file
    compose_file=$(resolve_compose_file "$suite")
    if [ -z "$compose_file" ]; then
        echo "❌ No docker-compose.yaml found for suite '$suite'"
        return 1
    fi

    generate_certs
    local project="pavis-${suite}-e2e"
    local compose_log="$PROJECT_ROOT/tests/temp/upstreams-${suite}.log"
    if [ "$quiet" -ne 1 ]; then
        echo "::group::🐳 Starting Shared Upstreams (${suite})"
    fi
    
    if docker compose -p "$project" -f "$compose_file" up -d --wait > "$compose_log" 2>&1; then
        if [ "$quiet" -ne 1 ]; then
            echo "✅ Upstreams started (Docker Compose)"
        fi
    else
        echo "❌ Failed to start upstreams!"
        cat "$compose_log"
        if [ "$quiet" -ne 1 ]; then
            echo "::endgroup::"
        fi
        return 1
    fi


    local unhealthy=0
    local services
    services=$(docker compose -p "$project" -f "$compose_file" ps --format "{{.Service}}")
    for svc in $services; do
        if ! docker compose -p "$project" -f "$compose_file" ps "$svc" | grep -q "Up"; then
             echo "⚠️ Service '$svc' is not Up."
             unhealthy=1
        fi
    done

    if [ "$unhealthy" -eq 1 ]; then
        echo "❌ One or more upstream services are unhealthy."
        cat "$compose_log"
        if [ "$quiet" -ne 1 ]; then
            echo "::endgroup::"
        fi
        return 1
    fi

    if [ "$quiet" -ne 1 ]; then
        echo "::endgroup::"
    fi
}

ensure_upstreams_docker() {
    local suite="$1"
    local compose_file
    compose_file=$(resolve_compose_file "$suite")
    if [ -z "$compose_file" ]; then
        return 1
    fi

    local project="pavis-${suite}-e2e"
    local services
    services=$(docker compose -p "$project" -f "$compose_file" ps --format "{{.Service}}" 2>/dev/null || true)
    if [ -z "$services" ]; then
        start_upstreams_docker "$suite" 1
        return $?
    fi

    local unhealthy=0
    for svc in $services; do
        if ! docker compose -p "$project" -f "$compose_file" ps "$svc" | grep -q "Up"; then
            unhealthy=1
            break
        fi
    done

    if [ "$unhealthy" -eq 1 ]; then
        start_upstreams_docker "$suite" 1
        return $?
    fi

    return 0
}

stop_upstreams_docker() {
    local suite="$1"
    local compose_file
    compose_file=$(resolve_compose_file "$suite")
    if [ -z "$compose_file" ]; then
        return
    fi
    local project="pavis-${suite}-e2e"
    if [ "${E2E_VERBOSE:-0}" -eq 1 ]; then
        echo "🛑 Stopping shared upstreams (${suite})..."
        docker compose -p "$project" -f "$compose_file" down -v
    else
        docker compose -p "$project" -f "$compose_file" down -v > /dev/null 2>&1
    fi

}

start_upstreams_binary() {
    if [ ! -x "$PAVIS_UPSTREAM_BIN" ]; then
        echo "❌ pavis-upstream binary not found at $PAVIS_UPSTREAM_BIN"
        echo "   Run 'cargo build --release -p pavis-upstream' before executing tests."
        return 1
    fi
    if ! can_bind_port; then
        echo "⏭️ Skipping binary upstreams (bind not permitted)."
        return 77
    fi

    generate_certs
    mkdir -p "$UPSTREAMS_PID_DIR" "$UPSTREAMS_LOG_DIR"
    rm -f "$UPSTREAMS_PID_DIR"/*.pid 2>/dev/null || true

    export UPSTREAM_HTTP_PORT_V1
    export UPSTREAM_HTTP_PORT_V2
    export UPSTREAM_HTTPS_PORT_V1
    export UPSTREAM_HTTPS_PORT_V2

    UPSTREAM_HTTP_PORT_V1=$(get_free_port)
    UPSTREAM_HTTP_PORT_V2=$(get_free_port)
    UPSTREAM_HTTPS_PORT_V1=$(get_free_port)
    UPSTREAM_HTTPS_PORT_V2=$(get_free_port)

    local cert_path="$CERTS_DIR/upstream_tls.pem"
    local key_path="$CERTS_DIR/upstream_tls.key"
    local instances=(
        "backend-v1:${UPSTREAM_HTTP_PORT_V1}:${UPSTREAM_HTTPS_PORT_V1}"
        "backend-v2:${UPSTREAM_HTTP_PORT_V2}:${UPSTREAM_HTTPS_PORT_V2}"
    )

    for entry in "${instances[@]}"; do
        IFS=: read -r name http_port https_port <<< "$entry"
        if ! launch_upstream_process "$name" "$http_port" "$https_port" "$cert_path" "$key_path"; then
            echo "❌ Failed to start upstream '$name'"
            stop_upstreams_binary
            cleanup_certs
            return 1
        fi
    done

    echo "✅ Upstreams started (binary mode)"
}

ensure_upstreams_binary() {
    local restart_needed=0

    if [ ! -d "$UPSTREAMS_PID_DIR" ]; then
        restart_needed=1
    else
        for port in "$UPSTREAM_HTTP_PORT_V1" "$UPSTREAM_HTTP_PORT_V2" "$UPSTREAM_HTTPS_PORT_V1" "$UPSTREAM_HTTPS_PORT_V2"; do
            [ -n "$port" ] || continue
            if ! wait_for_port "$port" 1; then
                restart_needed=1
                break
            fi
        done
    fi

    if [ "$restart_needed" -eq 1 ]; then
        stop_upstreams_binary
        start_upstreams_binary
    fi
}

launch_upstream_process() {
    local name="$1"
    local http_port="$2"
    local https_port="$3"
    local cert_path="$4"
    local key_path="$5"

    local log_file="$UPSTREAMS_LOG_DIR/${name}.log"
    
    # Build env array
    local env_vars=(
        "INSTANCE_ID=$name"
        "UPSTREAM_BIND_ADDR=${UPSTREAM_BIND_ADDR:-127.0.0.1}"
        "HTTP_PORT=$http_port"
        "TLS_CERT_FILE=$cert_path"
        "TLS_KEY_FILE=$key_path"
        "RUST_LOG=${UPSTREAM_LOG_LEVEL:-info}"
    )
    if [ -n "$https_port" ]; then
        env_vars+=("HTTPS_PORT=$https_port")
    fi

    env "${env_vars[@]}" "$PAVIS_UPSTREAM_BIN" > "$log_file" 2>&1 &
    local pid=$!
    echo "$pid" > "$UPSTREAMS_PID_DIR/$name.pid"

    if ! wait_for_port "$http_port" 15; then
        echo "⚠️ HTTP port $http_port for $name did not open in time"
        kill "$pid" >/dev/null 2>&1 || true
        rm -f "$UPSTREAMS_PID_DIR/$name.pid"
        return 1
    fi

    if [ -n "$https_port" ]; then
        if ! wait_for_port "$https_port" 15; then
            echo "⚠️ HTTPS port $https_port for $name did not open in time"
            kill "$pid" >/dev/null 2>&1 || true
            rm -f "$UPSTREAMS_PID_DIR/$name.pid"
            return 1
        fi
    fi

    return 0
}

stop_upstreams_binary() {
    if [ -d "$UPSTREAMS_PID_DIR" ]; then
        for pid_file in "$UPSTREAMS_PID_DIR"/*.pid; do
            [ -e "$pid_file" ] || continue
            local pid
            pid=$(cat "$pid_file")
            if kill -0 "$pid" 2>/dev/null; then
                kill "$pid" >/dev/null 2>&1 || true
                wait "$pid" 2>/dev/null || true
            fi
        done
        rm -rf "$UPSTREAMS_PID_DIR"
    fi

    if [ "${KEEP_UPSTREAM_LOGS:-0}" -ne 1 ]; then
        rm -rf "$UPSTREAMS_LOG_DIR"
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
        local cmd=("$PAVIS_BIN" "${args[@]}")
        if command -v stdbuf >/dev/null 2>&1; then
            cmd=(stdbuf -oL -eL "${cmd[@]}")
        fi
        RUST_LOG=debug "${cmd[@]}" > "$TEST_TMP/logs/${name}.log" 2>&1 &
        record_pid $! "$name"
    else
        local docker_args=(
            run -d --rm
            --user "$(id -u):$(id -g)"
            --network host
            -e RUST_LOG=debug
            -v "$TEST_TMP:$TEST_TMP:rw"
            -v "$CERTS_DIR:$CERTS_DIR:ro"
        )
        if [ -n "${PAVIS_ACCESS_LOG_CHANNEL_CAPACITY:-}" ]; then
            docker_args+=(-e "PAVIS_ACCESS_LOG_CHANNEL_CAPACITY=${PAVIS_ACCESS_LOG_CHANNEL_CAPACITY}")
        fi
        if [ -n "${PAVIS_ACCESS_LOG_WRITE_THROTTLE_MS:-}" ]; then
            docker_args+=(-e "PAVIS_ACCESS_LOG_WRITE_THROTTLE_MS=${PAVIS_ACCESS_LOG_WRITE_THROTTLE_MS}")
        fi
        local cmd_args=("--config" "$config_path")
        if [ -n "$relay_url" ]; then
            cmd_args+=("--relay-url" "$relay_url")
        fi

        local container_id
        container_id=$(docker "${docker_args[@]}" "$PAVIS_IMAGE" "${cmd_args[@]}")
        record_container "$container_id" "$name"
        docker logs -f "$container_id" > "$TEST_TMP/logs/${name}.log" 2>&1 &
        record_pid $! "${name}_logs"
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
        local container_id
        container_id=$(docker run -d --rm \
            --user "$(id -u):$(id -g)" \
            --network host \
            -e RUST_LOG=debug \
            -v "$TEST_TMP:$TEST_TMP:rw" \
            "$RELAY_IMAGE" \
            --config "$config_path")
        record_container "$container_id" "$name"
        docker logs -f "$container_id" > "$TEST_TMP/logs/${name}.log" 2>&1 &
        record_pid $! "${name}_logs"
    fi
}

run_mock_relay() {
    local port="$1"
    local name="${2:-mock-relay}"

    if [ "$TEST_MODE" == "binary" ]; then
        RUST_LOG=debug "$MOCK_RELAY_BIN" --listen "127.0.0.1:$port" > "$TEST_TMP/logs/${name}.log" 2>&1 &
        record_pid $! "$name"
    else
        local container_id
        container_id=$(docker run -d --rm \
            --user "$(id -u):$(id -g)" \
            --network host \
            -e RUST_LOG=debug \
            -e MOCK_RELAY_MODE="${MOCK_RELAY_MODE}" \
            -e MOCK_RELAY_TIMEOUT_MS="${MOCK_RELAY_TIMEOUT_MS:-30000}" \
            "$MOCK_RELAY_IMAGE" \
            --listen "0.0.0.0:$port")
        record_container "$container_id" "$name"
        docker logs -f "$container_id" > "$TEST_TMP/logs/${name}.log" 2>&1 &
        record_pid $! "${name}_logs"
    fi
}

publish_config() {
    local relay_url="$1"
    local pvs_path="$2"

    local timeout="${PAVIS_PUBLISH_TIMEOUT:-5}"
    local retries="${PAVIS_PUBLISH_RETRIES:-10}"
    local attempt=1

    while [ "$attempt" -le "$retries" ]; do
        if curl -f --connect-timeout 1 --max-time "$timeout" \
            -X POST "${relay_url}/publish" --data-binary "@${pvs_path}"; then
            return 0
        fi
        attempt=$((attempt + 1))
        sleep 0.2
    done

    echo "❌ publish_config failed after ${retries} attempts" >&2
    return 1
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
	listeners:
	  - name: "listener-$id"
	    address: "127.0.0.1:0"
	upstreams:
	  - name: "dummy-$id"
	    endpoints: []
	routes: []
	telemetry:
	  service_name: "relay-test-$id"
EOF
    "$PAVCTL_BIN" gen "$yaml_path" "$pvs_path"
}

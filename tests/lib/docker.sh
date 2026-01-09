#!/bin/bash

# tests/lib/docker.sh
# Manages shared upstream infrastructure (either Docker Compose or local binaries).

if [ -z "$PROJECT_ROOT" ]; then
    export PROJECT_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
fi

UPSTREAMS_COMPOSE="$PROJECT_ROOT/tests/config/upstreams.yaml"
SUITES_DIR="$PROJECT_ROOT/tests/suites"
UPSTREAMS_LOG="$PROJECT_ROOT/tests/temp/upstreams.log"
UPSTREAMS_PID_DIR="$PROJECT_ROOT/tests/temp/upstreams"
UPSTREAMS_LOG_DIR="$PROJECT_ROOT/tests/temp/upstream-logs"
mkdir -p "$PROJECT_ROOT/tests/temp"

# Assumes env.sh is sourced for generate_certs/cleanup_certs and wait_for_port helper

start_upstreams() {
    local suite="$1"
    if [ "$TEST_MODE" == "binary" ]; then
        start_upstreams_binary
    else
        start_upstreams_docker "$suite"
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
    if [ -f "$UPSTREAMS_COMPOSE" ]; then
        echo "$UPSTREAMS_COMPOSE"
    else
        echo ""
    fi
}

start_upstreams_docker() {
    local suite="$1"
    local compose_file=$(resolve_compose_file "$suite")
    if [ -z "$compose_file" ]; then
        echo "❌ No docker-compose.yaml found for suite '$suite'"
        return 1
    fi

    generate_certs
    local project="pavis-${suite:-shared}-e2e"
    local compose_log="$PROJECT_ROOT/tests/temp/upstreams-${suite:-shared}.log"
    echo "::group::🐳 Starting Shared Upstreams (${suite:-shared})"
    
    if docker compose -p "$project" -f "$compose_file" up -d --wait > "$compose_log" 2>&1; then
        echo "✅ Upstreams started (Docker Compose)"
    else
        echo "❌ Failed to start upstreams!"
        cat "$compose_log"
        echo "::endgroup::"
        return 1
    fi


    local unhealthy=0
    local services=$(docker compose -p "$project" -f "$compose_file" ps --format "{{.Service}}")
    for svc in $services; do
        if ! docker compose -p "$project" -f "$compose_file" ps "$svc" | grep -q "Up"; then
             echo "⚠️ Service '$svc' is not Up."
             unhealthy=1
        fi
    done

    if [ "$unhealthy" -eq 1 ]; then
        echo "❌ One or more upstream services are unhealthy."
        cat "$compose_log"
        echo "::endgroup::"
        return 1
    fi

    echo "::endgroup::"
}

stop_upstreams_docker() {
    local suite="$1"
    local compose_file=$(resolve_compose_file "$suite")
    if [ -z "$compose_file" ]; then
        return
    fi
    local project="pavis-${suite:-shared}-e2e"
    if [ "${E2E_VERBOSE:-0}" -eq 1 ]; then
        echo "🛑 Stopping shared upstreams (${suite:-shared})..."
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

    generate_certs
    mkdir -p "$UPSTREAMS_PID_DIR" "$UPSTREAMS_LOG_DIR"
    rm -f "$UPSTREAMS_PID_DIR"/*.pid 2>/dev/null || true

    local cert_path="$CERTS_DIR/upstream_tls.pem"
    local key_path="$CERTS_DIR/upstream_tls.key"
    local instances=(
        "backend-v1:8081:8443"
        "backend-v2:8082:"
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

launch_upstream_process() {
    local name="$1"
    local http_port="$2"
    local https_port="$3"
    local cert_path="$4"
    local key_path="$5"

    local log_file="$UPSTREAMS_LOG_DIR/${name}.log"
    INSTANCE_ID="$name" \
    HTTP_PORT="$http_port" \
    HTTPS_PORT="$https_port" \
    TLS_CERT_FILE="$cert_path" \
    TLS_KEY_FILE="$key_path" \
    RUST_LOG=${UPSTREAM_LOG_LEVEL:-info} \
        "$PAVIS_UPSTREAM_BIN" > "$log_file" 2>&1 &
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
            local pid=$(cat "$pid_file")
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

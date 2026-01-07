#!/bin/bash

# e2e/scripts/lib/process.sh

start_backend() {
    local port="$1"
    local text="$2"
    local pid_file="$3"
    local container_name="backend_${port}_${RANDOM}"

    # Start echo server using Docker
    docker run -d \
        --name "$container_name" \
        -p "127.0.0.1:${port}:8080" \
        -e "PORT=8080" \
        -e "ECHO_RESPONSE=$text" \
        ealen/echo-server > /dev/null

    # Store container name in pid file for cleanup
    echo "$container_name" > "$pid_file"
    wait_for_url "http://127.0.0.1:$port" 10
}

start_echo_backend() {
    local port="$1"
    local pid_file="$2"
    local container_name="echo_${port}_${RANDOM}"

    # ealen/echo-server returns full request details including path
    docker run -d \
        --name "$container_name" \
        -p "127.0.0.1:${port}:8080" \
        -e "PORT=8080" \
        ealen/echo-server > /dev/null

    echo "$container_name" > "$pid_file"
    wait_for_url "http://127.0.0.1:$port" 10
}

compose_up() {
    local compose_file="$1"
    local compose_dir=$(dirname "$compose_file")

    docker-compose -f "$compose_file" up -d

    # Store compose info for cleanup
    echo "$compose_file" > "${compose_dir}/.compose_active"
}

compose_down() {
    local compose_file="$1"
    local compose_dir=$(dirname "$compose_file")

    if [ -f "$compose_file" ]; then
        docker-compose -f "$compose_file" down -v --remove-orphans 2>/dev/null || true
    fi

    rm -f "${compose_dir}/.compose_active"
}

stop_pid() {
    local pid_file="$1"
    if [ -f "$pid_file" ]; then
        local identifier=$(cat "$pid_file")

        # Check if it's a Docker container name or a PID
        if docker ps -q -f "name=$identifier" 2>/dev/null | grep -q .; then
            # It's a Docker container
            docker stop "$identifier" > /dev/null 2>&1
            docker rm "$identifier" > /dev/null 2>&1
        else
            # It's a process PID
            if kill -0 "$identifier" 2>/dev/null; then
                kill "$identifier"
                wait "$identifier" 2>/dev/null
            fi
        fi
        rm "$pid_file"
    fi
}

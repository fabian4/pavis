#!/bin/bash
set -euo pipefail

# Docker utilities for shell scripts
# Provides Docker container management, health checks, and stats collection

_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$_LIB_DIR/log.sh"
source "$_LIB_DIR/process.sh"

# Check if docker is available and running
# Returns: 0 if docker is ready, 1 otherwise
require_docker() {
  if ! command -v docker &>/dev/null; then
    log_error "docker is required but not found. Please install Docker."
    return 1
  fi

  if ! docker info >/dev/null 2>&1; then
    log_error "docker daemon is not running. Please start Docker."
    return 1
  fi

  return 0
}

# Check if docker compose is available
# Returns: 0 if docker compose exists, 1 otherwise
require_docker_compose() {
  require_docker || return 1

  if ! docker compose version >/dev/null 2>&1; then
    log_error "docker compose is required (Docker 20.10+)"
    return 1
  fi

  return 0
}

# Check if a container is running
# Args:
#   $1 - Container ID or name
# Returns:
#   0 if container is running, 1 otherwise
docker_is_running() {
  local container="$1"

  if [[ -z "$container" ]]; then
    log_error "docker_is_running: Container ID or name required"
    return 1
  fi

  require_docker || return 1

  if docker inspect -f '{{.State.Running}}' "$container" 2>/dev/null | grep -q "^true$"; then
    log_debug "Container $container is running"
    return 0
  else
    log_debug "Container $container is not running"
    return 1
  fi
}

# Wait for container to become healthy
# Args:
#   $1 - Container ID or name
#   $2 - Timeout in seconds (default: 60)
# Returns:
#   0 if container becomes healthy, 1 on timeout or error
docker_wait_healthy() {
  local container="$1"
  local timeout="${2:-60}"

  if [[ -z "$container" ]]; then
    log_error "docker_wait_healthy: Container ID or name required"
    return 1
  fi

  require_docker || return 1

  log_debug "Waiting for container $container to become healthy (timeout: ${timeout}s)"

  local end=$(($(date +%s) + timeout))

  while [[ $(date +%s) -lt $end ]]; do
    # Check if container is running
    if ! docker_is_running "$container"; then
      log_error "Container $container is not running"
      return 1
    fi

    # Check health status
    local health_status
    health_status=$(docker inspect -f '{{.State.Health.Status}}' "$container" 2>/dev/null || echo "none")

    case "$health_status" in
      "healthy")
        log_debug "Container $container is healthy"
        return 0
        ;;
      "none")
        # No healthcheck defined, just check if running
        if docker_is_running "$container"; then
          log_debug "Container $container is running (no healthcheck defined)"
          return 0
        fi
        ;;
      "unhealthy")
        log_error "Container $container became unhealthy"
        return 1
        ;;
      "starting")
        log_debug "Container $container health status: starting"
        ;;
      *)
        log_debug "Container $container health status: $health_status"
        ;;
    esac

    sleep 2
  done

  log_error "Timeout waiting for container $container to become healthy"
  return 1
}

# Collect Docker stats for a container
# Args:
#   $1 - Container ID or name
#   $2 - Output CSV file
#   $3 - Duration in seconds (default: 10)
#   $4 - Interval in seconds (default: 1)
# Returns:
#   0 on success, 1 on error
docker_collect_stats() {
  local container="$1"
  local output_file="$2"
  local duration="${3:-10}"
  local interval="${4:-1}"

  if [[ -z "$container" || -z "$output_file" ]]; then
    log_error "docker_collect_stats: Container and output file required"
    return 1
  fi

  require_docker || return 1

  if ! docker_is_running "$container"; then
    log_error "Container $container is not running"
    return 1
  fi

  log_debug "Collecting stats for container $container (duration: ${duration}s, interval: ${interval}s)"

  # Create output directory
  mkdir -p "$(dirname "$output_file")"

  # Write CSV header
  echo "timestamp,cpu_percent,mem_usage_mib,mem_limit_mib,mem_percent,net_input_mb,net_output_mb" > "$output_file"

  local end=$(($(date +%s) + duration))
  while [[ $(date +%s) -lt $end ]]; do
    # Get stats in parseable format
    local stats_line
    stats_line=$(docker stats "$container" --no-stream --format "{{.CPUPerc}},{{.MemUsage}},{{.MemPerc}},{{.NetIO}}" 2>/dev/null || echo "")

    if [[ -n "$stats_line" ]]; then
      # Parse the stats line
      local timestamp
      timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

      # Remove % signs and units, extract values
      local cpu_percent
      cpu_percent=$(echo "$stats_line" | cut -d',' -f1 | sed 's/%//')

      local mem_usage
      mem_usage=$(echo "$stats_line" | cut -d',' -f2 | awk '{print $1}' | sed 's/MiB//')

      local mem_limit
      mem_limit=$(echo "$stats_line" | cut -d',' -f2 | awk '{print $3}' | sed 's/MiB//')

      local mem_percent
      mem_percent=$(echo "$stats_line" | cut -d',' -f3 | sed 's/%//')

      local net_input
      net_input=$(echo "$stats_line" | cut -d',' -f4 | awk '{print $1}' | sed 's/MB//')

      local net_output
      net_output=$(echo "$stats_line" | cut -d',' -f4 | awk '{print $3}' | sed 's/MB//')

      echo "$timestamp,$cpu_percent,$mem_usage,$mem_limit,$mem_percent,$net_input,$net_output" >> "$output_file"
    fi

    sleep "$interval"
  done

  log_debug "Stats collected to $output_file"
  return 0
}

# Stop and remove a container
# Args:
#   $1 - Container ID or name
#   $2 - Timeout for graceful stop (default: 10)
# Returns:
#   0 on success, 1 on error
docker_cleanup_container() {
  local container="$1"
  local timeout="${2:-10}"

  if [[ -z "$container" ]]; then
    log_error "docker_cleanup_container: Container ID or name required"
    return 1
  fi

  require_docker || return 1

  if ! docker inspect "$container" >/dev/null 2>&1; then
    log_debug "Container $container does not exist, nothing to clean up"
    return 0
  fi

  log_debug "Stopping and removing container $container"

  if docker_is_running "$container"; then
    if docker stop -t "$timeout" "$container" >/dev/null 2>&1; then
      log_debug "Container $container stopped"
    else
      log_warn "Failed to stop container $container gracefully, forcing kill"
      docker kill "$container" >/dev/null 2>&1 || true
    fi
  fi

  if docker rm -f "$container" >/dev/null 2>&1; then
    log_debug "Container $container removed"
    return 0
  else
    log_error "Failed to remove container $container"
    return 1
  fi
}

# Get container logs
# Args:
#   $1 - Container ID or name
#   $2 - Output file (optional, prints to stdout if not provided)
#   $3 - Number of lines (optional, "all" for full logs)
# Returns:
#   0 on success, 1 on error
docker_get_logs() {
  local container="$1"
  local output_file="${2:-}"
  local lines="${3:-all}"

  if [[ -z "$container" ]]; then
    log_error "docker_get_logs: Container ID or name required"
    return 1
  fi

  require_docker || return 1

  local docker_cmd="docker logs"
  if [[ "$lines" != "all" ]]; then
    docker_cmd="$docker_cmd --tail $lines"
  fi

  if [[ -n "$output_file" ]]; then
    mkdir -p "$(dirname "$output_file")"
    if $docker_cmd "$container" > "$output_file" 2>&1; then
      log_debug "Logs for container $container saved to $output_file"
      return 0
    else
      log_error "Failed to get logs for container $container"
      return 1
    fi
  else
    $docker_cmd "$container" 2>&1
    return $?
  fi
}

# Wait for a port to be available in a container
# Args:
#   $1 - Container ID or name
#   $2 - Port number
#   $3 - Timeout in seconds (default: 60)
# Returns:
#   0 if port becomes available, 1 on timeout
docker_wait_port() {
  local container="$1"
  local port="$2"
  local timeout="${3:-60}"

  if [[ -z "$container" || -z "$port" ]]; then
    log_error "docker_wait_port: Container and port required"
    return 1
  fi

  require_docker || return 1

  log_debug "Waiting for port $port in container $container (timeout: ${timeout}s)"

  # Get container IP
  local container_ip
  container_ip=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$container" 2>/dev/null)

  if [[ -z "$container_ip" ]]; then
    log_error "Could not get IP address for container $container"
    return 1
  fi

  local end=$(($(date +%s) + timeout))

  while [[ $(date +%s) -lt $end ]]; do
    if ! docker_is_running "$container"; then
      log_error "Container $container stopped running"
      return 1
    fi

    # Try to connect to the port
    if command -v nc &>/dev/null; then
      if nc -z "$container_ip" "$port" 2>/dev/null; then
        log_debug "Port $port is available in container $container"
        return 0
      fi
    elif [[ -e /dev/tcp ]]; then
      if bash -c "cat < /dev/tcp/$container_ip/$port" 2>/dev/null; then
        log_debug "Port $port is available in container $container"
        return 0
      fi
    else
      log_error "Neither nc nor /dev/tcp available for port check"
      return 2
    fi

    sleep 1
  done

  log_error "Timeout waiting for port $port in container $container"
  return 1
}

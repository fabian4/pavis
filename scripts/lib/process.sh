#!/bin/bash
set -euo pipefail

# Process management utilities for shell scripts
# Provides safe process lifecycle management, PID validation, and cleanup

_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$_LIB_DIR/log.sh"

# Check if a process is alive
# Args:
#   $1 - PID to check
# Returns:
#   0 if process exists, 1 if not
check_process_alive() {
  local pid="$1"

  if [[ -z "$pid" ]]; then
    log_error "check_process_alive: PID argument required"
    return 1
  fi

  if ! [[ "$pid" =~ ^[0-9]+$ ]]; then
    log_error "check_process_alive: Invalid PID '$pid'"
    return 1
  fi

  if kill -0 "$pid" 2>/dev/null; then
    log_debug "Process $pid is alive"
    return 0
  else
    log_debug "Process $pid is not running"
    return 1
  fi
}

# Safely kill a process with graceful degradation
# Args:
#   $1 - PID to kill
#   $2 - Timeout in seconds for graceful shutdown (default: 10)
#   $3 - Force kill if timeout (default: true)
# Returns:
#   0 if process terminated, 1 on error
kill_process_safe() {
  local pid="$1"
  local timeout="${2:-10}"
  local force="${3:-true}"

  if [[ -z "$pid" ]]; then
    log_error "kill_process_safe: PID argument required"
    return 1
  fi

  if ! [[ "$pid" =~ ^[0-9]+$ ]]; then
    log_error "kill_process_safe: Invalid PID '$pid'"
    return 1
  fi

  # Check if process exists
  if ! check_process_alive "$pid"; then
    log_debug "Process $pid already terminated"
    return 0
  fi

  # Send TERM signal for graceful shutdown
  log_debug "Sending TERM signal to process $pid"
  if ! kill -TERM "$pid" 2>/dev/null; then
    log_warn "Failed to send TERM signal to process $pid"
    return 1
  fi

  # Wait for graceful shutdown
  local elapsed=0
  while [[ $elapsed -lt $timeout ]]; do
    if ! check_process_alive "$pid"; then
      log_debug "Process $pid terminated gracefully after ${elapsed}s"
      return 0
    fi
    sleep 1
    ((elapsed++))
  done

  # Timeout reached
  if [[ "$force" == "true" ]]; then
    log_warn "Process $pid did not terminate gracefully, sending KILL signal"
    if kill -KILL "$pid" 2>/dev/null; then
      sleep 1
      if ! check_process_alive "$pid"; then
        log_debug "Process $pid force-killed successfully"
        return 0
      else
        log_error "Failed to force-kill process $pid"
        return 1
      fi
    else
      log_error "Failed to send KILL signal to process $pid"
      return 1
    fi
  else
    log_error "Process $pid did not terminate within ${timeout}s and force kill disabled"
    return 1
  fi
}

# Wait for a process to exit
# Args:
#   $1 - PID to wait for
#   $2 - Timeout in seconds (default: 60)
# Returns:
#   0 if process exited, 1 on timeout
wait_process_exit() {
  local pid="$1"
  local timeout="${2:-60}"

  if [[ -z "$pid" ]]; then
    log_error "wait_process_exit: PID argument required"
    return 1
  fi

  if ! [[ "$pid" =~ ^[0-9]+$ ]]; then
    log_error "wait_process_exit: Invalid PID '$pid'"
    return 1
  fi

  log_debug "Waiting for process $pid to exit (timeout: ${timeout}s)"

  local elapsed=0
  while [[ $elapsed -lt $timeout ]]; do
    if ! check_process_alive "$pid"; then
      log_debug "Process $pid exited after ${elapsed}s"
      return 0
    fi
    sleep 1
    ((elapsed++))
  done

  log_error "Timeout waiting for process $pid to exit"
  return 1
}

# Read and validate a PID from a file
# Args:
#   $1 - Path to PID file
# Returns:
#   0 and prints PID if valid, 1 on error
read_pid_file() {
  local pid_file="$1"

  if [[ -z "$pid_file" ]]; then
    log_error "read_pid_file: PID file path required"
    return 1
  fi

  if [[ ! -f "$pid_file" ]]; then
    log_error "PID file does not exist: $pid_file"
    return 1
  fi

  local pid
  pid=$(cat "$pid_file" 2>/dev/null)

  if [[ -z "$pid" ]]; then
    log_error "PID file is empty: $pid_file"
    return 1
  fi

  if ! [[ "$pid" =~ ^[0-9]+$ ]]; then
    log_error "Invalid PID in file $pid_file: '$pid'"
    return 1
  fi

  echo "$pid"
  return 0
}

# Kill process by PID file
# Args:
#   $1 - Path to PID file
#   $2 - Timeout in seconds for graceful shutdown (default: 10)
#   $3 - Remove PID file after kill (default: true)
# Returns:
#   0 if process terminated, 1 on error
kill_process_by_pidfile() {
  local pid_file="$1"
  local timeout="${2:-10}"
  local remove_file="${3:-true}"

  if [[ -z "$pid_file" ]]; then
    log_error "kill_process_by_pidfile: PID file path required"
    return 1
  fi

  local pid
  if ! pid=$(read_pid_file "$pid_file"); then
    return 1
  fi

  log_debug "Killing process $pid from PID file: $pid_file"

  if kill_process_safe "$pid" "$timeout"; then
    if [[ "$remove_file" == "true" ]]; then
      rm -f "$pid_file"
      log_debug "Removed PID file: $pid_file"
    fi
    return 0
  else
    return 1
  fi
}

# Get process name by PID
# Args:
#   $1 - PID
# Returns:
#   0 and prints process name, 1 on error
get_process_name() {
  local pid="$1"

  if [[ -z "$pid" ]]; then
    log_error "get_process_name: PID argument required"
    return 1
  fi

  if ! check_process_alive "$pid"; then
    log_error "Process $pid is not running"
    return 1
  fi

  if [[ "$(uname)" == "Darwin" ]]; then
    # macOS
    ps -p "$pid" -o comm= 2>/dev/null || return 1
  else
    # Linux
    ps -p "$pid" -o comm= 2>/dev/null || return 1
  fi
}

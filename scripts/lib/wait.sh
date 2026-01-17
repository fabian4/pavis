#!/bin/bash
set -euo pipefail

# Polling utilities for shell scripts

_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$_LIB_DIR/log.sh"

wait_for_url() {
  local url="$1"
  local timeout="$2"
  shift 2
  local end=$(($(date +%s) + timeout))

  while [[ $(date +%s) -lt $end ]]; do
    if curl -sf "$@" "$url" >/dev/null 2>&1; then
      log_debug "URL $url is ready"
      return 0
    fi
    sleep 2
  done

  log_error "Timeout waiting for URL $url"
  return 1
}

wait_for_port() {
  local host="$1"
  local port="$2"
  local timeout="$3"
  local end=$(($(date +%s) + timeout))

  if command -v nc &>/dev/null; then
    while [[ $(date +%s) -lt $end ]]; do
      if nc -z "$host" "$port" &>/dev/null; then
        log_debug "Port $host:$port is ready"
        return 0
      fi
      sleep 1
    done
  elif [[ -e /dev/tcp ]]; then
    while [[ $(date +%s) -lt $end ]]; do
      if bash -c "cat < /dev/tcp/$host/$port" 2>/dev/null; then
        log_debug "Port $host:$port is ready"
        return 0
      fi
      sleep 1
    done
  else
    log_error "Neither nc nor /dev/tcp available for port check"
    return 2
  fi

  log_error "Timeout waiting for port $host:$port"
  return 1
}

wait_for_file() {
  local filepath="$1"
  local timeout="$2"
  local end=$(($(date +%s) + timeout))

  while [[ $(date +%s) -lt $end ]]; do
    if [[ -f "$filepath" ]]; then
      log_debug "File $filepath exists"
      return 0
    fi
    sleep 1
  done

  log_error "Timeout waiting for file $filepath"
  return 1
}

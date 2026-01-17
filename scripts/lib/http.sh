#!/bin/bash
set -euo pipefail

# HTTP utilities for shell scripts
# Provides HTTP request helpers, status code validation, and response handling

_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$_LIB_DIR/log.sh"

# Perform HTTP GET request
# Args:
#   $1 - URL
#   $@ - Additional curl arguments
# Returns:
#   0 on success (2xx status), 1 on error
#   Prints response body to stdout
http_get() {
  local url="$1"
  shift

  if [[ -z "$url" ]]; then
    log_error "http_get: URL argument required"
    return 1
  fi

  log_debug "HTTP GET: $url"

  if ! command -v curl &>/dev/null; then
    log_error "curl is required but not found"
    return 1
  fi

  if curl -sSf "$@" "$url"; then
    return 0
  else
    local exit_code=$?
    log_error "HTTP GET failed for $url (curl exit code: $exit_code)"
    return 1
  fi
}

# Perform HTTP POST request
# Args:
#   $1 - URL
#   $2 - Data (optional, use - for stdin)
#   $@ - Additional curl arguments
# Returns:
#   0 on success (2xx status), 1 on error
#   Prints response body to stdout
http_post() {
  local url="$1"
  local data="${2:-}"
  shift 2 || shift 1

  if [[ -z "$url" ]]; then
    log_error "http_post: URL argument required"
    return 1
  fi

  log_debug "HTTP POST: $url"

  if ! command -v curl &>/dev/null; then
    log_error "curl is required but not found"
    return 1
  fi

  if [[ -n "$data" ]]; then
    if curl -sSf -X POST -d "$data" "$@" "$url"; then
      return 0
    else
      local exit_code=$?
      log_error "HTTP POST failed for $url (curl exit code: $exit_code)"
      return 1
    fi
  else
    if curl -sSf -X POST "$@" "$url"; then
      return 0
    else
      local exit_code=$?
      log_error "HTTP POST failed for $url (curl exit code: $exit_code)"
      return 1
    fi
  fi
}

# Check HTTP status code
# Args:
#   $1 - URL
#   $2 - Expected status code (default: 200)
#   $@ - Additional curl arguments
# Returns:
#   0 if status matches, 1 otherwise
#   Prints actual status code to stdout
check_http_status() {
  local url="$1"
  local expected_status="${2:-200}"
  shift 2 || shift 1

  if [[ -z "$url" ]]; then
    log_error "check_http_status: URL argument required"
    return 1
  fi

  if ! command -v curl &>/dev/null; then
    log_error "curl is required but not found"
    return 1
  fi

  local status_code
  status_code=$(curl -s -o /dev/null -w "%{http_code}" "$@" "$url")
  local curl_exit=$?

  if [[ $curl_exit -ne 0 ]]; then
    log_error "curl failed for $url (exit code: $curl_exit)"
    return 1
  fi

  echo "$status_code"

  if [[ "$status_code" == "$expected_status" ]]; then
    log_debug "HTTP status check passed: $status_code (expected: $expected_status)"
    return 0
  else
    log_warn "HTTP status mismatch: got $status_code, expected $expected_status"
    return 1
  fi
}

# Perform HTTP request and capture both status and body
# Args:
#   $1 - URL
#   $2 - Output file for body
#   $@ - Additional curl arguments (including -X for method)
# Returns:
#   0 on curl success, 1 on error
#   Prints HTTP status code to stdout
#   Writes response body to output file
http_request_full() {
  local url="$1"
  local output_file="$2"
  shift 2

  if [[ -z "$url" ]]; then
    log_error "http_request_full: URL argument required"
    return 1
  fi

  if [[ -z "$output_file" ]]; then
    log_error "http_request_full: Output file argument required"
    return 1
  fi

  if ! command -v curl &>/dev/null; then
    log_error "curl is required but not found"
    return 1
  fi

  log_debug "HTTP request: $url (output: $output_file)"

  local http_code
  http_code=$(curl -s -o "$output_file" -w "%{http_code}" "$@" "$url")
  local curl_status=$?

  if [[ $curl_status -ne 0 ]]; then
    log_error "curl failed for $url (exit code: $curl_status)"
    return 1
  fi

  echo "$http_code"
  return 0
}

# Wait for HTTP endpoint to return expected status
# Args:
#   $1 - URL
#   $2 - Timeout in seconds
#   $3 - Expected status code (default: 200)
#   $@ - Additional curl arguments
# Returns:
#   0 if endpoint responds with expected status, 1 on timeout
wait_for_http_status() {
  local url="$1"
  local timeout="$2"
  local expected_status="${3:-200}"
  shift 3 || shift 2

  if [[ -z "$url" || -z "$timeout" ]]; then
    log_error "wait_for_http_status: URL and timeout arguments required"
    return 1
  fi

  log_debug "Waiting for $url to return status $expected_status (timeout: ${timeout}s)"

  local end=$(($(date +%s) + timeout))

  while [[ $(date +%s) -lt $end ]]; do
    local status_code
    status_code=$(curl -s -o /dev/null -w "%{http_code}" "$@" "$url" 2>/dev/null || echo "000")

    if [[ "$status_code" == "$expected_status" ]]; then
      log_debug "Endpoint $url ready (status: $status_code)"
      return 0
    fi

    log_debug "Endpoint $url not ready (status: $status_code), retrying..."
    sleep 2
  done

  log_error "Timeout waiting for $url to return status $expected_status"
  return 1
}

# Check if URL is reachable (any 2xx or 3xx status)
# Args:
#   $1 - URL
#   $@ - Additional curl arguments
# Returns:
#   0 if reachable, 1 otherwise
is_url_reachable() {
  local url="$1"
  shift

  if [[ -z "$url" ]]; then
    log_error "is_url_reachable: URL argument required"
    return 1
  fi

  if ! command -v curl &>/dev/null; then
    log_error "curl is required but not found"
    return 1
  fi

  local status_code
  status_code=$(curl -s -o /dev/null -w "%{http_code}" "$@" "$url" 2>/dev/null || echo "000")

  # 2xx or 3xx status codes indicate reachable
  if [[ "$status_code" =~ ^[23][0-9]{2}$ ]]; then
    log_debug "URL $url is reachable (status: $status_code)"
    return 0
  else
    log_debug "URL $url is not reachable (status: $status_code)"
    return 1
  fi
}

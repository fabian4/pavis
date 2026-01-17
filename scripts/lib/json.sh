#!/bin/bash
set -euo pipefail

# JSON utilities for shell scripts
# Provides jq wrappers, validation, and JSON manipulation helpers

_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$_LIB_DIR/log.sh"

# Check if jq is available
# Returns: 0 if jq exists, 1 otherwise
require_jq() {
  if ! command -v jq &>/dev/null; then
    log_error "jq is required but not found. Please install jq."
    return 1
  fi
  return 0
}

# Validate JSON file or string
# Args:
#   $1 - Path to JSON file OR "-" for stdin
# Returns:
#   0 if valid JSON, 1 otherwise
json_validate() {
  local input="$1"

  if [[ -z "$input" ]]; then
    log_error "json_validate: Input argument required (file path or '-' for stdin)"
    return 1
  fi

  require_jq || return 1

  if [[ "$input" == "-" ]]; then
    # Validate stdin
    if jq empty >/dev/null 2>&1; then
      log_debug "JSON from stdin is valid"
      return 0
    else
      log_error "Invalid JSON from stdin"
      return 1
    fi
  elif [[ -f "$input" ]]; then
    # Validate file
    if jq empty "$input" >/dev/null 2>&1; then
      log_debug "JSON file is valid: $input"
      return 0
    else
      log_error "Invalid JSON in file: $input"
      return 1
    fi
  else
    log_error "JSON file not found: $input"
    return 1
  fi
}

# Extract a value from JSON file or string
# Args:
#   $1 - JSON file path OR "-" for stdin
#   $2 - jq query (e.g., ".field", ".nested.key")
#   $3 - Default value if key not found (optional)
# Returns:
#   0 and prints value, 1 on error
json_get() {
  local input="$1"
  local query="$2"
  local default="${3:-}"

  if [[ -z "$input" || -z "$query" ]]; then
    log_error "json_get: Input and query arguments required"
    return 1
  fi

  require_jq || return 1

  local result
  if [[ "$input" == "-" ]]; then
    # Read from stdin
    result=$(jq -r "${query} // empty" 2>/dev/null || echo "")
  elif [[ -f "$input" ]]; then
    # Read from file
    result=$(jq -r "${query} // empty" "$input" 2>/dev/null || echo "")
  else
    log_error "JSON file not found: $input"
    return 1
  fi

  if [[ -n "$result" ]]; then
    echo "$result"
    return 0
  elif [[ -n "$default" ]]; then
    echo "$default"
    return 0
  else
    log_debug "Key not found: $query (no default provided)"
    return 1
  fi
}

# Check if JSON file has required keys
# Args:
#   $1 - JSON file path
#   $@ - Required keys (space-separated)
# Returns:
#   0 if all keys exist, 1 otherwise
json_has_keys() {
  local input="$1"
  shift

  if [[ -z "$input" ]]; then
    log_error "json_has_keys: Input file required"
    return 1
  fi

  if [[ $# -eq 0 ]]; then
    log_error "json_has_keys: At least one key required"
    return 1
  fi

  require_jq || return 1

  if [[ ! -f "$input" ]]; then
    log_error "JSON file not found: $input"
    return 1
  fi

  local missing_keys=()
  for key in "$@"; do
    if ! jq -e "$key" "$input" >/dev/null 2>&1; then
      missing_keys+=("$key")
    fi
  done

  if [[ ${#missing_keys[@]} -gt 0 ]]; then
    log_error "Missing required keys in $input: ${missing_keys[*]}"
    return 1
  fi

  log_debug "All required keys present in $input"
  return 0
}

# Extract multiple values from JSON file
# Args:
#   $1 - JSON file path
#   $@ - Key names (will use as jq queries with ".")
# Returns:
#   0 and prints tab-separated values, 1 on error
json_get_multiple() {
  local input="$1"
  shift

  if [[ -z "$input" ]]; then
    log_error "json_get_multiple: Input file required"
    return 1
  fi

  if [[ $# -eq 0 ]]; then
    log_error "json_get_multiple: At least one key required"
    return 1
  fi

  require_jq || return 1

  if [[ ! -f "$input" ]]; then
    log_error "JSON file not found: $input"
    return 1
  fi

  local values=()
  for key in "$@"; do
    local value
    value=$(jq -r ".${key} // empty" "$input" 2>/dev/null || echo "")
    values+=("$value")
  done

  # Print tab-separated values
  local IFS=$'\t'
  echo "${values[*]}"
  return 0
}

# Pretty-print JSON file
# Args:
#   $1 - JSON file path OR "-" for stdin
# Returns:
#   0 and prints formatted JSON, 1 on error
json_pretty() {
  local input="$1"

  if [[ -z "$input" ]]; then
    log_error "json_pretty: Input argument required"
    return 1
  fi

  require_jq || return 1

  if [[ "$input" == "-" ]]; then
    jq '.' 2>/dev/null || return 1
  elif [[ -f "$input" ]]; then
    jq '.' "$input" 2>/dev/null || return 1
  else
    log_error "JSON file not found: $input"
    return 1
  fi
}

# Merge two JSON files
# Args:
#   $1 - Base JSON file
#   $2 - Override JSON file
# Returns:
#   0 and prints merged JSON, 1 on error
json_merge() {
  local base="$1"
  local override="$2"

  if [[ -z "$base" || -z "$override" ]]; then
    log_error "json_merge: Two JSON files required"
    return 1
  fi

  require_jq || return 1

  if [[ ! -f "$base" ]]; then
    log_error "Base JSON file not found: $base"
    return 1
  fi

  if [[ ! -f "$override" ]]; then
    log_error "Override JSON file not found: $override"
    return 1
  fi

  jq -s '.[0] * .[1]' "$base" "$override" 2>/dev/null || {
    log_error "Failed to merge JSON files"
    return 1
  }
}

# Convert JSON to shell-sourceable format (key=value)
# Args:
#   $1 - JSON file path
#   $2 - Prefix for variable names (optional)
# Returns:
#   0 and prints key=value pairs, 1 on error
json_to_env() {
  local input="$1"
  local prefix="${2:-}"

  if [[ -z "$input" ]]; then
    log_error "json_to_env: Input file required"
    return 1
  fi

  require_jq || return 1

  if [[ ! -f "$input" ]]; then
    log_error "JSON file not found: $input"
    return 1
  fi

  if [[ -n "$prefix" ]]; then
    jq -r "to_entries | .[] | \"${prefix}\(.key | ascii_upcase)=\(.value | @sh)\"" "$input" 2>/dev/null || return 1
  else
    jq -r 'to_entries | .[] | "\(.key | ascii_upcase)=\(.value | @sh)"' "$input" 2>/dev/null || return 1
  fi
}

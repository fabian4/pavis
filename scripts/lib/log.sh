#!/bin/bash
set -euo pipefail

# Shared logging primitives for shell scripts
# All output follows format "[LEVEL] message"

log_info() {
  echo "[INFO] $*"
}

log_warn() {
  echo "[WARN] $*"
}

log_error() {
  echo "[ERROR] $*" >&2
}

log_debug() {
  if [[ "${DEBUG:-0}" == "1" ]]; then
    echo "[DEBUG] $*"
  fi
}

log_section() {
  echo ""
  echo "=== $* ==="
  echo ""
}

exit_with_error() {
  local msg="$1"
  local code="${2:-1}"
  log_error "$msg"
  exit "$code"
}

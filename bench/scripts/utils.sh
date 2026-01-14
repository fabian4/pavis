#!/usr/bin/env bash
# Shared utility helpers for benchmark scripts.

# This file is intended to be sourced. Do not execute commands with side-effects
# at load time beyond function definitions.

# shellcheck disable=SC2034
BENCH_UTILS_LOADED=1

_timestamp() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

_log() {
  local level="$1"; shift
  printf '[%s] %s\n' "$level" "$*"
}

log_info() {
  _log INFO "$@"
}

log_warn() {
  _log WARN "$@"
}

log_error() {
  _log ERROR "$@" >&2
}

exit_with_error() {
  local msg="$1"; shift || true
  local code=${1:-1}
  log_error "$msg"
  exit "$code"
}

require_cmd() {
  local cmd
  for cmd in "$@"; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      exit_with_error "Missing required command: $cmd"
    fi
  done
}

ensure_dir() {
  local dir="$1"
  mkdir -p "$dir"
}

persist_env_var() {
  local name="$1" value="$2"
  if [[ -z "${BENCH_STATE_DIR:-}" ]]; then
    return
  fi
  ensure_dir "$BENCH_STATE_DIR"
  local state_file="$BENCH_STATE_DIR/bench.env"
  if [[ ! -f "$state_file" ]]; then
    touch "$state_file"
  fi
  # Remove existing line
  if grep -q "^export ${name}=" "$state_file"; then
    # Use temp file for portability
    local tmp
    tmp="${state_file}.tmp"
    grep -v "^export ${name}=" "$state_file" > "$tmp"
    mv "$tmp" "$state_file"
  fi
  printf 'export %s=%q\n' "$name" "$value" >> "$state_file"
}

load_persisted_env() {
  local state_file
  state_file="${BENCH_STATE_DIR:-}/bench.env"
  if [[ -n "$state_file" && -f "$state_file" ]]; then
    # shellcheck disable=SC1090
    source "$state_file"
  fi
}

collect_system_info() {
  local cpu kernel arch
  cpu="$(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d':' -f2- | sed 's/^ //' || true)"
  kernel="$(uname -r 2>/dev/null || true)"
  arch="$(uname -m 2>/dev/null || true)"
  printf 'cpu=%s\nkernel=%s\narch=%s\n' "${cpu:-unknown}" "${kernel:-unknown}" "${arch:-unknown}"
}

generate_log_filename() {
  local timestamp
  timestamp=$(date -u +"%Y%m%d_%H%M%S")
  echo "bench_${timestamp}.log"
}

is_background_mode() {
  [[ "${BENCH_BACKGROUND:-0}" == "1" || "${BENCH_BACKGROUND:-0}" == "true" ]]
}

print_background_info() {
  local log_file="$1"
  local pid="$2"
  printf '\n'
  printf '=== Background Mode Enabled ===\n'
  printf 'Process ID: %s\n' "$pid"
  printf 'Log file: %s\n' "$log_file"
  printf 'Monitor: tail -f %s\n' "$log_file"
  printf 'Stop: kill %s\n' "$pid"
  printf '\n'
  printf 'The benchmark will continue running even if SSH disconnects.\n'
  printf 'Check progress: tail -f %s\n' "$log_file"
  printf '\n'
}

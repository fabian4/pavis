#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"

check_requirements() {
  log_info "Checking required tooling"
  local required_commands=(
    bash
    docker
    jq
    awk
    sed
    mktemp
    curl
    python3
    cargo
    make
    git
  )
  require_cmd "${required_commands[@]}"

  if ! docker compose version >/dev/null 2>&1; then
    exit_with_error "docker compose is required (Docker 20.10+)"
  fi

  if [[ "${BENCH_PROFILE:-}" == "workstation" ]]; then
    if [[ "$(uname -s)" != "Linux" ]]; then
      log_warn "CPU pinning and memory limits are Linux-only; skipping taskset checks"
    else
      require_cmd taskset
    fi
  fi

  log_info "All required tools detected"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  check_requirements "$@"
fi

#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"

cleanup_environment() {
  load_persisted_env
  log_info "Cleaning up benchmark artifacts"

  if [[ "${BENCH_PROXY:-}" == "pavis" && "${BENCH_PVS_GENERATED:-false}" == "true" ]]; then
    if [[ -n "${BENCH_PVS_CONFIG:-}" && -f "$BENCH_PVS_CONFIG" ]]; then
      log_info "Removing generated PVS config"
      rm -f "$BENCH_PVS_CONFIG"
    fi
  fi

  if [[ -n "${BENCH_DOCKER_COMPOSE:-}" && -f "$BENCH_DOCKER_COMPOSE" ]]; then
    log_info "Ensuring docker compose stack is stopped"
    docker compose -f "$BENCH_DOCKER_COMPOSE" --profile sut down >/dev/null 2>&1 || true
  fi

  if [[ -n "${BENCH_STATE_DIR:-}" && -d "$BENCH_STATE_DIR" ]]; then
    rm -rf "$BENCH_STATE_DIR"
  fi
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  cleanup_environment "$@"
fi

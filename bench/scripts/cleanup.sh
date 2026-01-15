#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"

cleanup_system_mode() {
  log_info "Cleaning up system mode (Kubernetes) environment"

  local cluster_name="${BENCH_KIND_CLUSTER:-pavis-bench}"

  if command -v kind > /dev/null 2>&1; then
    if kind get clusters 2>/dev/null | grep -q "^${cluster_name}$"; then
      log_info "Deleting kind cluster: $cluster_name"
      kind delete cluster --name "$cluster_name" || true
    fi
  fi
}

cleanup_environment() {
  load_persisted_env

  # Skip cleanup on failure if BENCH_CLEANUP_ON_FAILURE=false (for CI debugging)
  if [[ "${BENCH_CLEANUP_ON_FAILURE:-true}" == "false" && "${BENCH_CLEANUP_FORCE:-false}" != "true" ]]; then
    log_info "Skipping cleanup (BENCH_CLEANUP_ON_FAILURE=false). Run 'make bench-system-down' to cleanup manually."
    return 0
  fi

  log_info "Cleaning up benchmark artifacts"

  # System mode cleanup
  if [[ "${BENCH_MODE:-standalone}" == "system" ]]; then
    cleanup_system_mode
    if [[ -n "${BENCH_STATE_DIR:-}" && -d "$BENCH_STATE_DIR" ]]; then
      rm -rf "$BENCH_STATE_DIR"
    fi
    return 0
  fi

  # Standalone mode cleanup
  if [[ "${BENCH_PROXY:-}" == "pavis" && "${BENCH_PVS_GENERATED:-false}" == "true" ]]; then
    if [[ -n "${BENCH_PVS_CONFIG:-}" && -f "$BENCH_PVS_CONFIG" ]]; then
      log_info "Removing generated PVS config"
      rm -f "$BENCH_PVS_CONFIG"
    fi
  fi

  if [[ -n "${BENCH_DOCKER_COMPOSE:-}" && -f "$BENCH_DOCKER_COMPOSE" ]]; then
    log_info "Ensuring docker compose stack is stopped"
    docker compose -f "$BENCH_DOCKER_COMPOSE" --profile sut down > /dev/null 2>&1 || true
  fi

  if [[ -n "${BENCH_STATE_DIR:-}" && -d "$BENCH_STATE_DIR" ]]; then
    rm -rf "$BENCH_STATE_DIR"
  fi
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  cleanup_environment "$@"
fi

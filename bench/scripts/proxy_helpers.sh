#!/usr/bin/env bash
set -euo pipefail

# Proxy Helpers for System Mode Tests (Pavis-only)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"

# Get pod label for current proxy
require_pavis_proxy() {
  if [[ "${BENCH_PROXY:-pavis}" != "pavis" ]]; then
    log_error "Unsupported BENCH_PROXY=${BENCH_PROXY:-}. This repository supports only pavis."
    return 1
  fi
}

get_proxy_pod_label() {
  require_pavis_proxy || return 1
  echo "app=test-backend"
}

# Get sidecar container name for current proxy
get_proxy_container_name() {
  require_pavis_proxy || return 1
  echo "pavis-sidecar"
}

# Get proxy port for current proxy
get_proxy_port() {
  require_pavis_proxy || return 1
  echo "8080"
}

# Get service name for current proxy
get_proxy_service_name() {
  require_pavis_proxy || return 1
  echo "test-backend"
}

# Check if proxy supports config versioning
proxy_supports_config_versioning() {
  require_pavis_proxy || return 1
  return 0
}

# Trigger config update for current proxy
proxy_trigger_config_update() {
  local version="$1"
  local drop_rate="${2:-0.0}"
  require_pavis_proxy || return 1
  # shellcheck source=bench/scripts/publish_config.sh
  source "$SCRIPT_DIR/publish_config.sh"
  trigger_config_update "$version" "$drop_rate"
}

# Deploy baseline config for current proxy
proxy_deploy_baseline_config() {
  require_pavis_proxy || return 1
  # shellcheck source=bench/scripts/publish_config.sh
  source "$SCRIPT_DIR/publish_config.sh"
  deploy_baseline_config
}

# Get proxy stats (RSS memory)
proxy_get_stats() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"
  require_pavis_proxy || return 1

  # shellcheck source=bench/scripts/k8s_helpers.sh
  source "$SCRIPT_DIR/k8s_helpers.sh"

  local container
  container=$(get_proxy_container_name)

  kubectl_get_container_stats "$label" "$container" "$namespace"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  log_info "proxy_helpers.sh loaded"
fi

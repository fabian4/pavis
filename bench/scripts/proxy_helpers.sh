#!/usr/bin/env bash
set -euo pipefail

# Proxy-Agnostic Helpers for System Mode Tests
# Provides abstraction layer for different proxy types

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"

# Get pod label for current proxy
get_proxy_pod_label() {
  local proxy="${BENCH_PROXY:-pavis}"

  case "$proxy" in
    pavis)
      echo "app=test-backend"
      ;;
    envoy)
      echo "app=envoy-test-backend"
      ;;
    linkerd)
      echo "app=linkerd-test-backend"
      ;;
    *)
      echo "app=test-backend"
      ;;
  esac
}

# Get sidecar container name for current proxy
get_proxy_container_name() {
  local proxy="${BENCH_PROXY:-pavis}"

  case "$proxy" in
    pavis)
      echo "pavis-sidecar"
      ;;
    envoy)
      echo "envoy-sidecar"
      ;;
    linkerd)
      echo "linkerd-proxy"
      ;;
    *)
      echo "pavis-sidecar"
      ;;
  esac
}

# Get proxy port for current proxy
get_proxy_port() {
  local proxy="${BENCH_PROXY:-pavis}"

  case "$proxy" in
    pavis)
      echo "8080"
      ;;
    envoy)
      echo "8080"
      ;;
    linkerd)
      echo "8081"  # Linkerd uses the app port directly
      ;;
    *)
      echo "8080"
      ;;
  esac
}

# Get service name for current proxy
get_proxy_service_name() {
  local proxy="${BENCH_PROXY:-pavis}"

  case "$proxy" in
    pavis)
      echo "test-backend"
      ;;
    envoy)
      echo "envoy-test-backend"
      ;;
    linkerd)
      echo "linkerd-test-backend"
      ;;
    *)
      echo "test-backend"
      ;;
  esac
}

# Check if proxy supports config versioning
proxy_supports_config_versioning() {
  local proxy="${BENCH_PROXY:-pavis}"

  case "$proxy" in
    pavis|envoy)
      return 0  # Supports versioning
      ;;
    linkerd)
      return 1  # Does not support versioning
      ;;
    *)
      return 1
      ;;
  esac
}

# Trigger config update for current proxy
proxy_trigger_config_update() {
  local version="$1"
  local drop_rate="${2:-0.0}"
  local proxy="${BENCH_PROXY:-pavis}"

  case "$proxy" in
    pavis)
      # shellcheck source=bench/scripts/publish_config.sh
      source "$SCRIPT_DIR/publish_config.sh"
      trigger_config_update "$version" "$drop_rate"
      ;;
    envoy)
      # shellcheck source=bench/scripts/publish_config.sh
      source "$SCRIPT_DIR/publish_config.sh"
      publish_to_envoy_xds
      ;;
    linkerd)
      log_warn "Linkerd does not support runtime config updates"
      return 1
      ;;
    *)
      log_error "Unknown proxy type: $proxy"
      return 1
      ;;
  esac
}

# Deploy baseline config for current proxy
proxy_deploy_baseline_config() {
  local proxy="${BENCH_PROXY:-pavis}"

  case "$proxy" in
    pavis)
      # shellcheck source=bench/scripts/publish_config.sh
      source "$SCRIPT_DIR/publish_config.sh"
      deploy_baseline_config
      ;;
    envoy)
      # shellcheck source=bench/scripts/publish_config.sh
      source "$SCRIPT_DIR/publish_config.sh"
      publish_to_envoy_xds
      ;;
    linkerd)
      # Linkerd doesn't need baseline config - it's always ready
      log_info "Linkerd proxy ready (no config deployment needed)"
      ;;
    *)
      log_error "Unknown proxy type: $proxy"
      return 1
      ;;
  esac
}

# Get proxy stats (RSS memory)
proxy_get_stats() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"
  local proxy="${BENCH_PROXY:-pavis}"

  # shellcheck source=bench/scripts/k8s_helpers.sh
  source "$SCRIPT_DIR/k8s_helpers.sh"

  local container
  container=$(get_proxy_container_name)

  kubectl_get_container_stats "$label" "$container" "$namespace"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  log_info "proxy_helpers.sh loaded"
fi

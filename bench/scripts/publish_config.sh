#!/usr/bin/env bash
set -euo pipefail

# Config Publishing Helpers for System Mode
# Provides functions for publishing configs to pavis-relay and envoy xDS

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"
# shellcheck source=bench/scripts/k8s_helpers.sh
source "$SCRIPT_DIR/k8s_helpers.sh"
# Source shared HTTP primitives
source "$SCRIPT_DIR/../../scripts/lib/http.sh"

RELAY_NAMESPACE="${BENCH_NAMESPACE:-bench-system}"

PAVIS_VERSION_HEADER="x-pavis-version"

resolve_pavctl_bin() {
  if [[ -n "${BENCH_PAVCTL_BIN:-}" && -x "${BENCH_PAVCTL_BIN}" ]]; then
    echo "${BENCH_PAVCTL_BIN}"
    return 0
  fi

  local pavctl="${BENCH_ROOT}/target/release/pavctl"
  if [[ -x "$pavctl" ]]; then
    echo "$pavctl"
    return 0
  fi

  log_info "Building pavctl" >&2
  cargo build -p pavctl --release
  echo "$pavctl"
}

build_pvs_from_config() {
  local config_file="$1"
  local output_file="$2"
  local pavctl
  pavctl="$(resolve_pavctl_bin)"
  "$pavctl" gen "$config_file" "$output_file"
}

kubectl_port_forward_service_background() {
  local service="$1"
  local local_port="$2"
  local remote_port="$3"
  local namespace="${4:-${BENCH_NAMESPACE:-bench-system}}"

  local attempt=0
  local max_attempts=5
  local chosen_port="$local_port"
  local pf_pid=""

  while [[ $attempt -lt $max_attempts ]]; do
    if [[ $attempt -gt 0 ]]; then
      chosen_port=$(pick_free_port "$local_port")
    fi

    kubectl port-forward -n "$namespace" "svc/${service}" "$chosen_port:$remote_port" \
      > /dev/null 2>&1 &
    pf_pid=$!

    sleep 2

    if check_process_alive "$pf_pid"; then
      echo "$pf_pid $chosen_port"
      return 0
    fi

    attempt=$((attempt + 1))
  done

  return 1
}

# Publish config to pavis-relay
# Usage: publish_to_pavis_relay <config_file> <version>
publish_to_pavis_relay() {
  local config_file="$1"
  local version="$2"

  if [[ ! -f "$config_file" ]]; then
    log_error "Config file not found: $config_file"
    return 1
  fi

  # Get relay service IP or use port-forward
  local relay_url
  if command -v kubectl > /dev/null 2>&1; then
    local pf_pid=""
    local pf_port="${BENCH_RELAY_LOCAL_PORT:-8090}"
    local pf_local_port="$pf_port"
    if kubectl_wait_for_endpoint "pavis-relay" "$RELAY_NAMESPACE" 30; then
      local pf_info=""
      pf_info=$(kubectl_port_forward_background "app=pavis-relay" "$pf_port" 8090 "$RELAY_NAMESPACE" || true)
      if [[ -n "$pf_info" ]]; then
        pf_pid=$(echo "$pf_info" | awk '{print $1}')
        pf_local_port=$(echo "$pf_info" | awk '{print $2}')
      fi
    fi
    if [[ -n "$pf_pid" ]]; then
      relay_url="http://localhost:${pf_local_port}/v1/publish"
    else
      local relay_ip
      relay_ip=$(kubectl_get_service_ip "pavis-relay" "$RELAY_NAMESPACE")
      relay_url="http://${relay_ip}:8090/v1/publish"
    fi
  else
    relay_url="http://localhost:8090/v1/publish"
  fi

  local temp_pvs
  temp_pvs=$(mktemp --suffix=.pvs)
  build_pvs_from_config "$config_file" "$temp_pvs"

  local temp_response
  temp_response=$(mktemp --suffix=.txt)

  local http_code
  local attempt_version="$version"
  local attempt=0

  while (( attempt < 2 )); do
    # Use http_request_full from scripts/lib/http.sh
    http_code=$(http_request_full "$relay_url" "$temp_response" \
      -X POST \
      -H "Content-Type: application/octet-stream" \
      -H "${PAVIS_VERSION_HEADER}: ${attempt_version}" \
      --data-binary "@${temp_pvs}")

    local request_status=$?

    if (( request_status != 0 )); then
      log_error "Failed to publish config (HTTP request failed)"
      break
    fi

    if [[ "$http_code" == "200" ]]; then
      log_info "Published config version ${attempt_version} to pavis-relay"
      PAVIS_PUBLISHED_VERSION="${attempt_version}"
      export PAVIS_PUBLISHED_VERSION
      rm -f "$temp_response"
      rm -f "$temp_pvs"
      if [[ -n "${pf_pid:-}" ]]; then
        kubectl_stop_port_forward "$pf_pid"
      fi
      return 0
    fi

    if [[ "$http_code" == "409" && $attempt -eq 0 ]]; then
      local current_version
      current_version=$(grep -oE 'current=[0-9]+' "$temp_response" | head -n1 | cut -d= -f2 || true)
      if [[ -n "$current_version" ]]; then
        attempt_version=$((current_version + 1))
      else
        attempt_version=$((attempt_version + 1))
      fi
      attempt=$((attempt + 1))
      continue
    fi

    log_error "Failed to publish config (status ${http_code}): $(cat "$temp_response")"
    break
  done

  rm -f "$temp_response"
  rm -f "$temp_pvs"

  if [[ -n "${pf_pid:-}" ]]; then
    kubectl_stop_port_forward "$pf_pid"
  fi
  return 1
}

publish_envoy_xds_snapshot() {
  local publish_mode="${1:-valid}"
  local pf_pid=""
  local xds_url
  if command -v kubectl > /dev/null 2>&1; then
    local pf_port="${BENCH_XDS_LOCAL_PORT:-18080}"
    local pf_local_port="$pf_port"
    if kubectl_wait_for_endpoint "envoy-xds" "$RELAY_NAMESPACE" 30; then
      local pf_info=""
      pf_info=$(kubectl_port_forward_service_background "envoy-xds" "$pf_port" 8080 "$RELAY_NAMESPACE" || true)
      if [[ -z "$pf_info" ]]; then
        log_warn "Service port-forward failed; trying pod port-forward"
        pf_info=$(kubectl_port_forward_background "app=envoy-xds" "$pf_port" 8080 "$RELAY_NAMESPACE" || true)
      fi
      if [[ -n "$pf_info" ]]; then
        read -r pf_pid pf_local_port <<<"$pf_info"
      fi
      if [[ ! "$pf_pid" =~ ^[0-9]+$ || ! "$pf_local_port" =~ ^[0-9]+$ ]]; then
        pf_pid=""
        pf_local_port="$pf_port"
      fi
    fi
    if [[ -n "$pf_pid" ]]; then
      xds_url="http://localhost:${pf_local_port}/v1/publish"
    else
      log_error "Failed to establish port-forward to envoy-xds"
      return 1
    fi
  else
    xds_url="http://localhost:8080/v1/publish"
  fi

  if [[ -z "$xds_url" ]]; then
    log_error "envoy-xds publish URL is empty"
    return 1
  fi
  if [[ ! "$xds_url" =~ ^http://[^/]+:[0-9]+/v1/publish$ ]]; then
    log_error "envoy-xds publish URL is invalid: ${xds_url}"
    return 1
  fi
  local url="${xds_url}"
  if [[ "$publish_mode" == "invalid" ]]; then
    url="${xds_url}?mode=invalid"
  fi
  log_info "Publishing envoy xDS snapshot via ${url}"

  local response
  local curl_status
  set +e
  response=$(curl -s --max-time 5 -X POST "$url" 2>&1)
  curl_status=$?
  set -e

  if [[ -n "$pf_pid" ]]; then
    kubectl_stop_port_forward "$pf_pid"
  fi

  if (( curl_status != 0 )); then
    log_error "Failed to publish xDS snapshot (curl exit ${curl_status}): $response"
    return 1
  fi

  if echo "$response" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    local version
    version=$(echo "$response" | jq -r '.version')
    log_info "Published envoy xDS snapshot version $version"
    return 0
  else
    log_error "Failed to publish xDS snapshot: $response"
    return 1
  fi
}

# Publish snapshot to envoy xDS server
# Usage: publish_to_envoy_xds
publish_to_envoy_xds() {
  publish_envoy_xds_snapshot "valid"
}

# Generate test config for pavis
# Usage: generate_pavis_config <version> <output_file> [drop_rate]
generate_pavis_config() {
  local version="$1"
  local output_file="$2"
  local drop_rate="${3:-0.0}"

  local config_file
  config_file=$(resolve_pavis_config_path)
  cp "$config_file" "$output_file"
  if [[ "$drop_rate" != "0.0" && "$drop_rate" != "0" ]]; then
    log_warn "drop_rate=$drop_rate ignored for current pavis config schema"
  fi
  log_info "Generated pavis config version $version"
}

resolve_pavis_config_path() {
  if [[ "${BENCH_MODE:-standalone}" == "system" ]]; then
    echo "${BENCH_ROOT}/bench/config/system/pavis/pavis.yaml"
  else
    echo "${BENCH_ROOT}/bench/config/standalone/pavis.yaml"
  fi
}

# Deploy baseline config (version 1, no drops)
# Usage: deploy_baseline_config
deploy_baseline_config() {
  local temp_config
  temp_config=$(mktemp --suffix=.yaml)

  generate_pavis_config 1 "$temp_config" 0.0
  publish_to_pavis_relay "$temp_config" 1

  rm -f "$temp_config"

  # Wait for config to be active
  sleep 2
}

# Deploy degraded config (version with high drop rate)
# Usage: deploy_degraded_config <version> <drop_rate>
deploy_degraded_config() {
  local version="$1"
  local drop_rate="${2:-1.0}"

  local temp_config
  temp_config=$(mktemp --suffix=.yaml)

  generate_pavis_config "$version" "$temp_config" "$drop_rate"
  publish_to_pavis_relay "$temp_config" "$version"

  rm -f "$temp_config"
}

# Rollback to previous config version
# Usage: rollback_config <target_version>
rollback_config() {
  local target_version="$1"

  local temp_config
  temp_config=$(mktemp --suffix=.yaml)

  generate_pavis_config "$target_version" "$temp_config" 0.0
  publish_to_pavis_relay "$temp_config" "$target_version"

  rm -f "$temp_config"

  log_info "Rolled back to config version $target_version"
}

# Trigger config update
# Usage: trigger_config_update <new_version>
trigger_config_update() {
  local new_version="$1"
  local drop_rate="${2:-0.0}"

  local temp_config
  temp_config=$(mktemp --suffix=.yaml)

  generate_pavis_config "$new_version" "$temp_config" "$drop_rate"
  publish_to_pavis_relay "$temp_config" "$new_version"

  rm -f "$temp_config"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  log_info "publish_config.sh loaded"
fi

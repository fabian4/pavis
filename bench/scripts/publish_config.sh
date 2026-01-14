#!/usr/bin/env bash
set -euo pipefail

# Config Publishing Helpers for System Mode
# Provides functions for publishing configs to pavis-relay and envoy xDS

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"
# shellcheck source=bench/scripts/k8s_helpers.sh
source "$SCRIPT_DIR/k8s_helpers.sh"

RELAY_NAMESPACE="${BENCH_NAMESPACE:-bench-system}"

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
    local relay_ip
    relay_ip=$(kubectl_get_service_ip "pavis-relay" "$RELAY_NAMESPACE")
    relay_url="http://${relay_ip}:8090/v1/publish"
  else
    relay_url="http://localhost:8090/v1/publish"
  fi

  # Publish config with version
  local response
  response=$(curl -s -X POST "$relay_url" \
    -H "Content-Type: application/json" \
    -d "{\"version\": $version, \"config\": $(cat "$config_file" | jq -Rs .)}" \
    2>&1)

  if echo "$response" | jq -e '.status == "ok"' > /dev/null 2>&1; then
    log_info "Published config version $version to pavis-relay"
    return 0
  else
    log_error "Failed to publish config: $response"
    return 1
  fi
}

# Publish snapshot to envoy xDS server
# Usage: publish_to_envoy_xds
publish_to_envoy_xds() {
  local xds_url
  if command -v kubectl > /dev/null 2>&1; then
    local xds_ip
    xds_ip=$(kubectl_get_service_ip "envoy-xds" "$RELAY_NAMESPACE")
    xds_url="http://${xds_ip}:8080/v1/publish"
  else
    xds_url="http://localhost:8080/v1/publish"
  fi

  local response
  response=$(curl -s -X POST "$xds_url" 2>&1)

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

# Generate test config for pavis
# Usage: generate_pavis_config <version> <output_file> [drop_rate]
generate_pavis_config() {
  local version="$1"
  local output_file="$2"
  local drop_rate="${3:-0.0}"

  cat > "$output_file" <<EOF
{
  "version": $version,
  "routes": [
    {
      "match": {
        "path_prefix": "/"
      },
      "action": {
        "upstream": "127.0.0.1:8081",
        "drop_rate": $drop_rate
      }
    }
  ]
}
EOF

  log_info "Generated pavis config version $version (drop_rate=$drop_rate)"
}

# Deploy baseline config (version 1, no drops)
# Usage: deploy_baseline_config
deploy_baseline_config() {
  local temp_config
  temp_config=$(mktemp)

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
  temp_config=$(mktemp)

  generate_pavis_config "$version" "$temp_config" "$drop_rate"
  publish_to_pavis_relay "$temp_config" "$version"

  rm -f "$temp_config"
}

# Rollback to previous config version
# Usage: rollback_config <target_version>
rollback_config() {
  local target_version="$1"

  local temp_config
  temp_config=$(mktemp)

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
  temp_config=$(mktemp)

  generate_pavis_config "$new_version" "$temp_config" "$drop_rate"
  publish_to_pavis_relay "$temp_config" "$new_version"

  rm -f "$temp_config"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  log_info "publish_config.sh loaded"
fi

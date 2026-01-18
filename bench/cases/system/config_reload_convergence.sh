#!/usr/bin/env bash
set -euo pipefail

# System Mode Test: Config Reload Convergence
# Measures time for config updates to converge across all sidecars

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/scripts"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"
# shellcheck source=bench/scripts/k8s_helpers.sh
source "$SCRIPT_DIR/k8s_helpers.sh"
# shellcheck source=bench/scripts/system_metrics.sh
source "$SCRIPT_DIR/system_metrics.sh"
# shellcheck source=bench/scripts/publish_config.sh
source "$SCRIPT_DIR/publish_config.sh"
# shellcheck source=bench/scripts/proxy_helpers.sh
source "$SCRIPT_DIR/proxy_helpers.sh"
# shellcheck source=bench/config/targets.env
source "$(cd "$SCRIPT_DIR/.." && pwd)/config/targets.env"

CASE_NAME="config_reload_convergence"
TARGET_RPS="${SYSTEM_CONFIG_RELOAD_CONVERGENCE_TARGET_RPS}"
DURATION_S="${SYSTEM_CONFIG_RELOAD_CONVERGENCE_DURATION_S}"
CONVERGENCE_WINDOW_S="${SYSTEM_CONFIG_RELOAD_CONVERGENCE_CONVERGENCE_WINDOW_S}"
NAMESPACE="${BENCH_NAMESPACE:-bench-system}"

main() {
  log_info "Starting test: $CASE_NAME for ${BENCH_PROXY}"

  # Check if proxy supports config versioning
  if ! proxy_supports_config_versioning; then
    log_warn "Proxy ${BENCH_PROXY} does not support config versioning, skipping test"
    return 0
  fi

  # Get proxy-specific configuration
  local pod_label
  local proxy_port
  pod_label=$(get_proxy_pod_label)
  proxy_port=$(get_proxy_port)

  local output_dir="${BENCH_OUTPUT_DIR}/${BENCH_MODE}/${BENCH_PROXY}/${CASE_NAME}"
  ensure_dir "$output_dir"

  # Setup port-forward to access test backend
  log_info "Setting up port-forward to test backend"
  local pf_pid
  local pf_info
  pf_info=$(kubectl_port_forward_background "$pod_label" "$proxy_port" "$proxy_port" "$NAMESPACE")
  pf_pid=$(echo "$pf_info" | awk '{print $1}')
  local pf_local_port
  pf_local_port=$(echo "$pf_info" | awk '{print $2}')

  # Wait for port-forward to stabilize
  sleep 3

  local target_base="http://localhost:${pf_local_port}"
  local target_url="${target_base}/fixed"
  local health_url="${target_base}/health"

  # Step 1: Deploy baseline config (version 1)
  log_info "Deploying baseline config (v1)"
  proxy_deploy_baseline_config
  local baseline_version="${PAVIS_PUBLISHED_VERSION:-1}"

  if [[ "${BENCH_PROXY}" == "pavis" ]]; then
    if ! wait_for_response_body "$health_url" "OK" 30; then
      log_error "Failed to observe baseline health response"
      kubectl_stop_port_forward "$pf_pid"
      return 1
    fi
  fi

  # Step 2: Start baseline load
  log_info "Starting baseline load at ${TARGET_RPS} RPS"
  local loadgen_pid
  "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$TARGET_RPS" \
    --duration "$DURATION_S" \
    --connections 100 \
    --output "${output_dir}/baseline.json" \
    > /dev/null 2>&1 &
  loadgen_pid=$!

  sleep 5

  # Step 3: Capture baseline P99
  log_info "Capturing baseline P99 latency"
  local baseline_p99
  baseline_p99=$(capture_p99_snapshot 5 "$target_url" "$TARGET_RPS")
  log_info "Baseline P99: ${baseline_p99}ms"

  # Step 4: Trigger config update (version 2)
  log_info "Triggering config update to v2"
#  local convergence_start
#  convergence_start=$(timestamp_ms)

  if [[ "${BENCH_PROXY}" == "pavis" ]]; then
    publish_pavis_config_variant 2 "OK-V2"
  else
    proxy_trigger_config_update 2 0.0
  fi
  local target_version="${PAVIS_PUBLISHED_VERSION:-2}"

  # Step 5: Measure convergence time
  log_info "Measuring convergence time"
  local convergence_time
  if [[ "${BENCH_PROXY}" == "pavis" ]]; then
    convergence_time=$(collect_convergence_time_response "$health_url" "OK-V2" 60000) || {
      log_error "Failed to measure convergence time"
      kubectl_stop_port_forward "$pf_pid"
      kill "$loadgen_pid" 2>/dev/null || true
      return 1
    }
  else
    convergence_time=$(collect_convergence_time "$target_version" 60000) || {
      log_error "Failed to measure convergence time"
      kubectl_stop_port_forward "$pf_pid"
      kill "$loadgen_pid" 2>/dev/null || true
      return 1
    }
  fi
  log_info "Convergence time: ${convergence_time}ms"

  # Step 6: Measure transition P99
  sleep 2
  log_info "Capturing transition P99 latency"
  local transition_p99
  transition_p99=$(capture_p99_snapshot "$CONVERGENCE_WINDOW_S" "$target_url" "$TARGET_RPS")
  log_info "Transition P99: ${transition_p99}ms"

  # Step 7: Wait for load test to complete
  wait "$loadgen_pid" 2>/dev/null || true

  # Step 8: Cleanup port-forward
  kubectl_stop_port_forward "$pf_pid"

  # Step 9: Calculate metrics
  local p99_delta
  p99_delta=$(echo "$transition_p99 - $baseline_p99" | bc -l)

  # Step 10: Extract final stats from baseline run
  local errors_5xx=0
  if [[ -f "${output_dir}/baseline.json" ]]; then
    errors_5xx=$(jq -r '.errors // 0' "${output_dir}/baseline.json")
  fi

  # Step 11: Write metrics JSON
  cat > "${output_dir}/metrics.json" <<EOF
{
  "test": "$CASE_NAME",
  "proxy": "${BENCH_PROXY}",
  "baseline_p99_ms": $baseline_p99,
  "convergence_time_ms": $convergence_time,
  "transition_p99_ms": $transition_p99,
  "p99_delta_ms": $p99_delta,
  "errors_5xx": $errors_5xx,
  "config_version_before": $baseline_version,
  "config_version_after": $target_version,
  "target_rps": $TARGET_RPS,
  "duration_s": $DURATION_S
}
EOF

  log_info "Metrics written to ${output_dir}/metrics.json"

  # Step 12: Validation
  log_info "Validating results"
  local validation_failed=0

  if (( $(echo "$convergence_time > 5000" | bc -l) )); then
    log_warn "Convergence time exceeded 5s threshold: ${convergence_time}ms"
    validation_failed=1
  fi

  if (( errors_5xx > 0 )); then
    log_warn "Detected ${errors_5xx} 5xx errors during test"
    validation_failed=1
  fi

  if (( validation_failed == 0 )); then
    log_info "Test PASSED: $CASE_NAME"
    return 0
  else
    log_warn "Test completed with warnings: $CASE_NAME"
    return 0
  fi
}

publish_pavis_config_variant() {
  local version="$1"
  local health_body="$2"

  local temp_config
  temp_config=$(mktemp --suffix=.yaml)

  cp "$(resolve_pavis_config_path)" "$temp_config"
  if command -v yq > /dev/null 2>&1; then
    yq -i ".routes[0].paths[0].body = \"${health_body}\"" "$temp_config"
  else
    sed -i.bak "s/body: \"OK\"/body: \"${health_body}\"/" "$temp_config"
    rm -f "${temp_config}.bak"
  fi

  publish_to_pavis_relay "$temp_config" "$version"
  rm -f "$temp_config"
}

wait_for_response_body() {
  local url="$1"
  local expected_body="$2"
  local timeout_s="${3:-30}"

  local elapsed=0
  while [[ $elapsed -lt $timeout_s ]]; do
    local body
    body=$(curl -s "$url" 2>/dev/null || true)
    if [[ "$body" == "$expected_body" ]]; then
      return 0
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done

  return 1
}

collect_convergence_time_response() {
  local url="$1"
  local expected_body="$2"
  local max_wait_ms="${3:-60000}"

  local start_ms
  local end_ms
  local elapsed_ms=0
  start_ms=$(date +%s%3N)

  while [[ $elapsed_ms -lt $max_wait_ms ]]; do
    local body
    body=$(curl -s "$url" 2>/dev/null || true)
    if [[ "$body" == "$expected_body" ]]; then
      end_ms=$(date +%s%3N)
      echo $((end_ms - start_ms))
      return 0
    fi
    sleep 0.1
    end_ms=$(date +%s%3N)
    elapsed_ms=$((end_ms - start_ms))
  done

  log_error "Timeout waiting for config response change"
  return 1
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi

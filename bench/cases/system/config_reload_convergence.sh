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
# shellcheck source=bench/config/config.env
source "$(cd "$SCRIPT_DIR/.." && pwd)/config/config.env"
# shellcheck source=scripts/lib/http.sh
source "$SCRIPT_DIR/../../scripts/lib/http.sh"

CASE_NAME="config_reload_convergence"
TARGET_RPS="${SYSTEM_CONFIG_RELOAD_CONVERGENCE_TARGET_RPS}"
DURATION_S="${SYSTEM_CONFIG_RELOAD_CONVERGENCE_DURATION_S}"
CONVERGENCE_WINDOW_S="${SYSTEM_CONFIG_RELOAD_CONVERGENCE_CONVERGENCE_WINDOW_S}"
NAMESPACE="${BENCH_NAMESPACE:-bench-system}"

format_float_3() {
  printf "%.3f" "$1"
}

format_float_3_or_empty() {
  local value="$1"
  if [[ -z "$value" ]]; then
    echo ""
    return 0
  fi
  printf "%.3f" "$value"
}

main() {
  log_info "Starting test: $CASE_NAME for ${BENCH_PROXY}"

  # Get proxy-specific configuration
  local pod_label
  local proxy_port
  local container_name
  pod_label=$(get_proxy_pod_label)
  proxy_port=$(get_proxy_port)
  container_name=$(get_proxy_container_name)

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
  if [[ -z "$pf_pid" || -z "$pf_local_port" ]]; then
    log_error "Port-forward to test backend failed"
    return 1
  fi

  # Wait for port-forward to stabilize
  sleep 3

  local target_base="http://localhost:${pf_local_port}"
  local target_url="${target_base}/fixed"
  local health_url="${target_base}/health"

  # Step 1: Deploy baseline config (version 1)
  log_info "Deploying baseline config (v1)"
  proxy_deploy_baseline_config
  local baseline_version="${PAVIS_PUBLISHED_VERSION:-1}"

  if ! wait_for_response_body "$health_url" "OK" 30; then
    log_error "Failed to observe baseline health response"
    kubectl_stop_port_forward "$pf_pid"
    return 1
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
    > /dev/null 2> "${output_dir}/baseline.err" &
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

  publish_pavis_config_variant 2 "OK-V2"
  local target_version="${PAVIS_PUBLISHED_VERSION:-2}"

  # Step 5: Measure convergence time
  log_info "Measuring convergence time"
  local convergence_time
  convergence_time=$(collect_convergence_time_response "$health_url" "OK-V2" 60000) || {
    log_error "Failed to measure convergence time"
    kubectl_stop_port_forward "$pf_pid"
    kill "$loadgen_pid" 2>/dev/null || true
    return 1
  }
  log_info "Convergence time: ${convergence_time}ms"

  # Step 6: Measure transition P99
  sleep 2
  log_info "Capturing transition P99 latency"
  local transition_p99
  transition_p99=$(capture_p99_snapshot "$CONVERGENCE_WINDOW_S" "$target_url" "$TARGET_RPS")
  log_info "Transition P99: ${transition_p99}ms"

  # Step 7: Wait for load test to complete
  local loadgen_status=0
  wait "$loadgen_pid" 2>/dev/null || loadgen_status=$?
  if (( loadgen_status != 0 )); then
    log_error "bench-loadgen failed during baseline load (see ${output_dir}/baseline.err)"
    kubectl_stop_port_forward "$pf_pid"
    return 1
  fi

  # Step 8: Cleanup port-forward
  kubectl_stop_port_forward "$pf_pid"

  # Step 9: Calculate metrics
  local p99_delta
  p99_delta=$(echo "$transition_p99 - $baseline_p99" | bc -l)
  if [[ "$p99_delta" == .* ]]; then
    p99_delta="0${p99_delta}"
  fi

  # Step 10: Extract final stats from baseline run
  local errors_5xx=0
  if [[ -f "${output_dir}/baseline.json" ]]; then
    errors_5xx=$(jq -r '.errors // 0' "${output_dir}/baseline.json")
  fi
  local achieved_rps=""
  if [[ -f "${output_dir}/baseline.json" ]]; then
    achieved_rps=$(jq -r '.achieved_rps // empty' "${output_dir}/baseline.json")
  fi

  local baseline_p99_fmt
  local convergence_time_fmt
  local transition_p99_fmt
  local p99_delta_fmt
  local errors_5xx_fmt
  local config_version_before_fmt
  local config_version_after_fmt
  local target_rps_fmt
  local achieved_rps_fmt
  local duration_s_fmt
  baseline_p99_fmt=$(format_float_3 "$baseline_p99")
  convergence_time_fmt=$(format_float_3 "$convergence_time")
  transition_p99_fmt=$(format_float_3 "$transition_p99")
  p99_delta_fmt=$(format_float_3 "$p99_delta")
  errors_5xx_fmt=$(format_float_3 "$errors_5xx")
  config_version_before_fmt=$(format_float_3 "$baseline_version")
  config_version_after_fmt=$(format_float_3 "$target_version")
  target_rps_fmt=$(format_float_3 "$TARGET_RPS")
  achieved_rps_fmt=$(format_float_3_or_empty "$achieved_rps")
  duration_s_fmt=$(format_float_3 "$DURATION_S")

  # Step 11: Write metrics JSON
  cat > "${output_dir}/metrics.json" <<EOF
{
  "test": "$CASE_NAME",
  "proxy": "${BENCH_PROXY}",
  "baseline_p99_ms": $baseline_p99_fmt,
  "convergence_time_ms": $convergence_time_fmt,
  "transition_p99_ms": $transition_p99_fmt,
  "p99_delta_ms": $p99_delta_fmt,
  "errors_5xx": $errors_5xx_fmt,
  "config_version_before": $config_version_before_fmt,
  "config_version_after": $config_version_after_fmt,
  "target_rps": $target_rps_fmt,
  "achieved_rps": $achieved_rps_fmt,
  "duration_s": $duration_s_fmt
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

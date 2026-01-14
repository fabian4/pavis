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

CASE_NAME="config_reload_convergence"
TARGET_RPS=1000
DURATION_S=60
CONVERGENCE_WINDOW_S=10
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
#  local container_name
  local proxy_port
  pod_label=$(get_proxy_pod_label)
#  container_name=$(get_proxy_container_name)
  proxy_port=$(get_proxy_port)

  local output_dir="${BENCH_OUTPUT_DIR}/${BENCH_MODE}/${BENCH_PROXY}/${CASE_NAME}"
  ensure_dir "$output_dir"

  # Setup port-forward to access test backend
  log_info "Setting up port-forward to test backend"
  local pf_pid
  pf_pid=$(kubectl_port_forward_background "$pod_label" "$proxy_port" "$proxy_port" "$NAMESPACE")

  # Wait for port-forward to stabilize
  sleep 3

  local target_url="http://localhost:${proxy_port}/fixed"

  # Step 1: Deploy baseline config (version 1)
  log_info "Deploying baseline config (v1)"
  proxy_deploy_baseline_config

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

  proxy_trigger_config_update 2 0.0

  # Step 5: Measure convergence time
  log_info "Measuring convergence time"
  local convergence_time
  convergence_time=$(collect_convergence_time 2 60000) || {
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
  "config_version_before": 1,
  "config_version_after": 2,
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

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi

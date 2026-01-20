#!/usr/bin/env bash
set -euo pipefail

# System Mode Test: Stress Recovery
# Measures proxy behavior during saturation and recovery

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

CASE_NAME="stress_recovery"
BASELINE_RPS="${SYSTEM_STRESS_RECOVERY_BASELINE_RPS}"
STRESS_RPS="${SYSTEM_STRESS_RECOVERY_STRESS_RPS}"
BASELINE_DURATION_S="${SYSTEM_STRESS_RECOVERY_BASELINE_DURATION_S}"
STRESS_DURATION_S="${SYSTEM_STRESS_RECOVERY_STRESS_DURATION_S}"
RECOVERY_DURATION_S="${SYSTEM_STRESS_RECOVERY_RECOVERY_DURATION_S}"
NAMESPACE="${BENCH_NAMESPACE:-bench-system}"

numeric_or_zero() {
  local raw="$1"
  # Extract first valid floating point number or integer
  local cleaned
  cleaned=$(echo "$raw" | grep -oE '([0-9]+\.?[0-9]*|\.[0-9]+)' | head -1)
  if [[ -z "$cleaned" ]]; then
    echo "0"
  else
    echo "$cleaned"
  fi
}

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

is_number() {
  local value="$1"
  [[ "$value" =~ ^-?[0-9]+([.][0-9]+)?([eE][-+]?[0-9]+)?$ ]]
}

main() {
  log_info "Starting test: $CASE_NAME for ${BENCH_PROXY}"

  # Get proxy-specific configuration
  local pod_label
  local container_name
  local proxy_port
  pod_label=$(get_proxy_pod_label)
  container_name=$(get_proxy_container_name)
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
  if [[ -z "$pf_pid" || -z "$pf_local_port" ]]; then
    log_error "Port-forward to test backend failed"
    return 1
  fi

  # Wait for port-forward to stabilize
  sleep 3

  local target_url="http://localhost:${pf_local_port}/fixed"

  # Step 1: Deploy baseline config
  log_info "Deploying baseline config"
  proxy_deploy_baseline_config

  sleep 2
  if ! wait_for_http_status "$target_url" 30 200; then
    log_error "Test backend did not become ready at ${target_url}"
    kubectl_stop_port_forward "$pf_pid"
    return 1
  fi

  # Step 2: Establish baseline at 50% load
  log_info "Establishing baseline at ${BASELINE_RPS} RPS (50% load)"
  if ! "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$BASELINE_RPS" \
    --duration "$BASELINE_DURATION_S" \
    --connections 100 \
    --output "${output_dir}/baseline.json" \
    > /dev/null 2> "${output_dir}/baseline.err"; then
    log_error "bench-loadgen failed during baseline load (see ${output_dir}/baseline.err)"
    kubectl_stop_port_forward "$pf_pid"
    return 1
  fi

  local baseline_p99
  baseline_p99=$(jq -r '.latency_ms.p99' "${output_dir}/baseline.json")
  if ! is_number "$baseline_p99" || (( $(echo "$baseline_p99 <= 0" | bc -l) )); then
    log_error "Invalid baseline P99: '${baseline_p99}'"
    kubectl_stop_port_forward "$pf_pid"
    return 1
  fi
  local baseline_rss_start_raw
  baseline_rss_start_raw=$(proxy_get_stats "$pod_label" "$NAMESPACE")
  log_info "Debug: Raw Baseline RSS: '${baseline_rss_start_raw}'"
  local baseline_rss_start
  baseline_rss_start=$(numeric_or_zero "$baseline_rss_start_raw")
  log_info "Baseline P99: ${baseline_p99}ms, RSS: ${baseline_rss_start}KB"

  # Step 3: Apply 150% saturation load
  log_info "Applying stress load at ${STRESS_RPS} RPS (150% saturation)"

  # Start RSS monitoring in background
  collect_rss_timeline "$STRESS_DURATION_S" 5 "${output_dir}/stress_rss.csv" "$pod_label" "$container_name" "$NAMESPACE" &
  local rss_monitor_pid=$!

  if ! "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$STRESS_RPS" \
    --duration "$STRESS_DURATION_S" \
    --connections 200 \
    --output "${output_dir}/stress.json" \
    > /dev/null 2> "${output_dir}/stress.err"; then
    log_error "bench-loadgen failed during stress load (see ${output_dir}/stress.err)"
    kubectl_stop_port_forward "$pf_pid"
    return 1
  fi

  wait "$rss_monitor_pid" 2>/dev/null || true

  local stress_p99
  stress_p99=$(jq -r '.latency_ms.p99' "${output_dir}/stress.json")
  local stress_dropped
  stress_dropped=$(jq -r '.dropped // 0' "${output_dir}/stress.json")
  local stress_rss_peak_raw
  stress_rss_peak_raw=$(awk -F',' 'NR>1 {if($2>max)max=$2} END{print max}' "${output_dir}/stress_rss.csv")
  log_info "Debug: Raw Stress RSS Peak: '${stress_rss_peak_raw}'"
  local stress_rss_peak
  stress_rss_peak=$(numeric_or_zero "$stress_rss_peak_raw")
  log_info "Stress P99: ${stress_p99}ms, Dropped: ${stress_dropped}, RSS Peak: ${stress_rss_peak}KB"

  # Step 4: Return to baseline load
  log_info "Returning to baseline load (${BASELINE_RPS} RPS)"

  # Start recovery RSS monitoring
  collect_rss_timeline "$RECOVERY_DURATION_S" 5 "${output_dir}/recovery_rss.csv" "$pod_label" "$container_name" "$NAMESPACE" &
  local recovery_rss_pid=$!

  if ! "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$BASELINE_RPS" \
    --duration "$RECOVERY_DURATION_S" \
    --connections 100 \
    --output "${output_dir}/recovery.json" \
    > /dev/null 2> "${output_dir}/recovery.err"; then
    log_error "bench-loadgen failed during recovery load (see ${output_dir}/recovery.err)"
    kubectl_stop_port_forward "$pf_pid"
    return 1
  fi

  wait "$recovery_rss_pid" 2>/dev/null || true

  local recovery_p99
  recovery_p99=$(jq -r '.latency_ms.p99' "${output_dir}/recovery.json")
  if ! is_number "$recovery_p99" || (( $(echo "$recovery_p99 <= 0" | bc -l) )); then
    log_error "Invalid recovery P99: '${recovery_p99}'"
    kubectl_stop_port_forward "$pf_pid"
    return 1
  fi
  local recovery_rss_end_raw
  recovery_rss_end_raw=$(awk -F',' 'END{print $2}' "${output_dir}/recovery_rss.csv")
  log_info "Debug: Raw Recovery RSS End: '${recovery_rss_end_raw}'"
  local recovery_rss_end
  recovery_rss_end=$(numeric_or_zero "$recovery_rss_end_raw")
  log_info "Recovery P99: ${recovery_p99}ms, RSS End: ${recovery_rss_end}KB"

  # Cleanup port-forward
  kubectl_stop_port_forward "$pf_pid"

  # Step 5: Calculate metrics
  local rss_growth
  rss_growth=$(echo "($recovery_rss_end - $baseline_rss_start) / 1024.0" | bc -l)
  local rss_growth_pct
  if (( $(echo "$baseline_rss_start <= 0" | bc -l) )); then
    rss_growth_pct=0
    log_warn "Skipping RSS growth percent: baseline RSS unavailable"
  else
    rss_growth_pct=$(echo "($recovery_rss_end - $baseline_rss_start) * 100.0 / $baseline_rss_start" | bc -l)
  fi

  # Percent delta from baseline after recovery (smaller is better).
  local latency_regression_pct
  latency_regression_pct=$(echo "($recovery_p99 - $baseline_p99) * 100.0 / $baseline_p99" | bc -l)

  local baseline_achieved_rps=""
  local stress_achieved_rps=""
  local recovery_achieved_rps=""
  if [[ -f "${output_dir}/baseline.json" ]]; then
    baseline_achieved_rps=$(jq -r '.achieved_rps // empty' "${output_dir}/baseline.json")
  fi
  if [[ -f "${output_dir}/stress.json" ]]; then
    stress_achieved_rps=$(jq -r '.achieved_rps // empty' "${output_dir}/stress.json")
  fi
  if [[ -f "${output_dir}/recovery.json" ]]; then
    recovery_achieved_rps=$(jq -r '.achieved_rps // empty' "${output_dir}/recovery.json")
  fi

  local baseline_rps_fmt
  local stress_rps_fmt
  local baseline_achieved_rps_fmt
  local stress_achieved_rps_fmt
  local recovery_achieved_rps_fmt
  local baseline_p99_fmt
  local stress_p99_fmt
  local recovery_p99_fmt
  local latency_regression_pct_fmt
  local stress_dropped_fmt
  local baseline_rss_fmt
  local stress_rss_peak_fmt
  local recovery_rss_fmt
  local rss_growth_fmt
  local rss_growth_pct_fmt
  baseline_rps_fmt=$(format_float_3 "$BASELINE_RPS")
  stress_rps_fmt=$(format_float_3 "$STRESS_RPS")
  baseline_achieved_rps_fmt=$(format_float_3_or_empty "$baseline_achieved_rps")
  stress_achieved_rps_fmt=$(format_float_3_or_empty "$stress_achieved_rps")
  recovery_achieved_rps_fmt=$(format_float_3_or_empty "$recovery_achieved_rps")
  baseline_p99_fmt=$(format_float_3 "$baseline_p99")
  stress_p99_fmt=$(format_float_3 "$stress_p99")
  recovery_p99_fmt=$(format_float_3 "$recovery_p99")
  latency_regression_pct_fmt=$(format_float_3 "$latency_regression_pct")
  stress_dropped_fmt=$(format_float_3 "$stress_dropped")
  baseline_rss_fmt=$(format_float_3 "$baseline_rss_start")
  stress_rss_peak_fmt=$(format_float_3 "$stress_rss_peak")
  recovery_rss_fmt=$(format_float_3 "$recovery_rss_end")
  rss_growth_fmt=$(format_float_3 "$rss_growth")
  rss_growth_pct_fmt=$(format_float_3 "$rss_growth_pct")

  # Step 6: Write metrics JSON
  cat > "${output_dir}/metrics.json" <<EOF
{
  "test": "$CASE_NAME",
  "proxy": "${BENCH_PROXY}",
  "baseline_rps": $baseline_rps_fmt,
  "stress_rps": $stress_rps_fmt,
  "baseline_achieved_rps": $baseline_achieved_rps_fmt,
  "stress_achieved_rps": $stress_achieved_rps_fmt,
  "recovery_achieved_rps": $recovery_achieved_rps_fmt,
  "baseline_p99_ms": $baseline_p99_fmt,
  "stress_p99_ms": $stress_p99_fmt,
  "recovery_p99_ms": $recovery_p99_fmt,
  "latency_regression_pct": $latency_regression_pct_fmt,
  "stress_dropped": $stress_dropped_fmt,
  "baseline_rss_kb": $baseline_rss_fmt,
  "stress_rss_peak_kb": $stress_rss_peak_fmt,
  "recovery_rss_kb": $recovery_rss_fmt,
  "rss_growth_mb": $rss_growth_fmt,
  "rss_growth_pct": $rss_growth_pct_fmt
}
EOF

  log_info "Metrics written to ${output_dir}/metrics.json"

  # Step 7: Validation
  log_info "Validating results"
  local validation_failed=0

  # Check if latency recovered to within 20% of baseline
  if (( $(echo "$latency_regression_pct > 20" | bc -l) )); then
    log_warn "Latency regression too high: ${latency_regression_pct}% above baseline"
    validation_failed=1
  fi

  # Check for excessive RSS growth (>10%)
  if (( $(echo "$baseline_rss_start > 0" | bc -l) )); then
    if (( $(echo "$rss_growth_pct > 10" | bc -l) )); then
      log_warn "Excessive RSS growth detected: ${rss_growth_pct}%"
      validation_failed=1
    fi
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

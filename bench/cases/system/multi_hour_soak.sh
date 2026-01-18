#!/usr/bin/env bash
set -euo pipefail

# System Mode Test: Multi-Hour Soak Test
# Validates long-term stability under sustained load

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

CASE_NAME="multi_hour_soak"
TARGET_RPS="${SYSTEM_MULTI_HOUR_SOAK_TARGET_RPS}"  # 75% capacity
DURATION_HOURS="${SOAK_DURATION_HOURS:-${SYSTEM_MULTI_HOUR_SOAK_DURATION_HOURS}}"
DURATION_S=$((DURATION_HOURS * 3600))
SAMPLE_INTERVAL_S="${SYSTEM_MULTI_HOUR_SOAK_SAMPLE_INTERVAL_S}"
NAMESPACE="${BENCH_NAMESPACE:-bench-system}"

main() {
  log_info "Starting test: $CASE_NAME (${DURATION_HOURS}h soak test) for ${BENCH_PROXY}"

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

  # Wait for port-forward to stabilize
  sleep 3

  local target_url="http://localhost:${pf_local_port}/fixed"

  # Step 1: Deploy baseline config
  log_info "Deploying baseline config"
  proxy_deploy_baseline_config

  sleep 2

  # Step 2: Capture initial metrics
  log_info "Capturing initial metrics"
  local rss_start
  rss_start=$(proxy_get_stats "$pod_label" "$NAMESPACE" | tr -d 'Ki')
  local fd_start
  fd_start=$(collect_fd_count "$pod_label" "$container_name" "$NAMESPACE")
  log_info "Initial RSS: ${rss_start}KB, FD: ${fd_start}"

  # Step 3: Start background resource monitoring
  log_info "Starting resource monitoring (interval: ${SAMPLE_INTERVAL_S}s)"
  collect_rss_timeline "$DURATION_S" "$SAMPLE_INTERVAL_S" "${output_dir}/rss_timeline.csv" "$pod_label" "$container_name" "$NAMESPACE" &
  local rss_monitor_pid=$!

  collect_fd_timeline "$DURATION_S" "$SAMPLE_INTERVAL_S" "${output_dir}/fd_timeline.csv" "$pod_label" "$container_name" "$NAMESPACE" &
  local fd_monitor_pid=$!

  # Step 4: Run sustained load test
  log_info "Running ${DURATION_HOURS}h soak test at ${TARGET_RPS} RPS (75% capacity)"
  log_info "This will take approximately ${DURATION_HOURS} hours to complete..."

  "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$TARGET_RPS" \
    --duration "$DURATION_S" \
    --connections 150 \
    --output "${output_dir}/soak.json" \
    > "${output_dir}/loadgen.log" 2>&1

  log_info "Soak test completed"

  # Wait for resource monitors to finish
  wait "$rss_monitor_pid" 2>/dev/null || true
  wait "$fd_monitor_pid" 2>/dev/null || true

  # Step 5: Capture final metrics
  log_info "Capturing final metrics"
  local rss_end
  rss_end=$(proxy_get_stats "$pod_label" "$NAMESPACE" | tr -d 'Ki')
  local fd_end
  fd_end=$(collect_fd_count "$pod_label" "$container_name" "$NAMESPACE")
  log_info "Final RSS: ${rss_end}KB, FD: ${fd_end}"

  # Cleanup port-forward
  kubectl_stop_port_forward "$pf_pid"

  # Step 6: Calculate RSS slope (memory leak detection)
  log_info "Calculating RSS slope"
  local rss_slope_mb_per_hour
  rss_slope_mb_per_hour=$(calculate_rss_slope "${output_dir}/rss_timeline.csv")
  log_info "RSS slope: ${rss_slope_mb_per_hour} MB/hour"

  # Step 7: Extract load test metrics
  local requests_ok
  requests_ok=$(jq -r '.requests_ok' "${output_dir}/soak.json")
  local errors
  errors=$(jq -r '.errors' "${output_dir}/soak.json")
  local p99
  p99=$(jq -r '.latency_ms.p99' "${output_dir}/soak.json")
  local achieved_rps
  achieved_rps=$(jq -r '.achieved_rps' "${output_dir}/soak.json")

  # Step 8: Calculate stability metrics
  local rss_growth_mb
  rss_growth_mb=$(echo "($rss_end - $rss_start) / 1024.0" | bc -l)
  local fd_delta
  fd_delta=$((fd_end - fd_start))
  local fd_stable
  if (( fd_delta >= -5 && fd_delta <= 5 )); then
    fd_stable=true
  else
    fd_stable=false
  fi

  # Step 9: Write metrics JSON
  cat > "${output_dir}/metrics.json" <<EOF
{
  "test": "$CASE_NAME",
  "proxy": "${BENCH_PROXY}",
  "duration_hours": $DURATION_HOURS,
  "target_rps": $TARGET_RPS,
  "achieved_rps": $achieved_rps,
  "requests_ok": $requests_ok,
  "errors": $errors,
  "p99_ms": $p99,
  "rss_start_kb": $rss_start,
  "rss_end_kb": $rss_end,
  "rss_growth_mb": $rss_growth_mb,
  "rss_slope_mb_per_hour": $rss_slope_mb_per_hour,
  "fd_start": $fd_start,
  "fd_end": $fd_end,
  "fd_delta": $fd_delta,
  "fd_stable": $fd_stable
}
EOF

  log_info "Metrics written to ${output_dir}/metrics.json"

  # Step 10: Validation
  log_info "Validating results"
  local validation_failed=0

  # Check for memory leak (>2 MB/hour slope)
  if (( $(echo "$rss_slope_mb_per_hour > 2.0" | bc -l) )); then
    log_warn "Potential memory leak detected: ${rss_slope_mb_per_hour} MB/hour"
    validation_failed=1
  fi

  # Check FD stability
  if [[ "$fd_stable" == "false" ]]; then
    log_warn "File descriptor count not stable: delta=$fd_delta"
    validation_failed=1
  fi

  # Check error rate (<0.01%)
  local error_rate
  error_rate=$(echo "scale=6; $errors * 100.0 / ($requests_ok + $errors)" | bc -l)
  if (( $(echo "$error_rate > 0.01" | bc -l) )); then
    log_warn "Error rate too high: ${error_rate}%"
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

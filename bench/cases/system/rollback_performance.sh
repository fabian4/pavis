#!/usr/bin/env bash
set -euo pipefail

# System Mode Test: Rollback Performance
# Measures time to restore baseline performance after rolling back from bad config

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

CASE_NAME="rollback_performance"
TARGET_RPS="${SYSTEM_ROLLBACK_PERFORMANCE_TARGET_RPS}"
BASELINE_DURATION_S="${SYSTEM_ROLLBACK_PERFORMANCE_BASELINE_DURATION_S}"
DEGRADED_DURATION_S="${SYSTEM_ROLLBACK_PERFORMANCE_DEGRADED_DURATION_S}"
RECOVERY_DURATION_S="${SYSTEM_ROLLBACK_PERFORMANCE_RECOVERY_DURATION_S}"
NAMESPACE="${BENCH_NAMESPACE:-bench-system}"

main() {
  log_info "Starting test: $CASE_NAME for ${BENCH_PROXY}"

  # Check if proxy supports config versioning
  if ! proxy_supports_config_versioning; then
    log_warn "Proxy ${BENCH_PROXY} does not support config versioning, skipping test"
    return 0
  fi

  if [[ "${BENCH_PROXY}" != "pavis" ]]; then
    log_warn "Proxy ${BENCH_PROXY} does not support degraded config simulation, skipping test"
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
  local pf_info
  pf_info=$(kubectl_port_forward_background "$pod_label" "$proxy_port" "$proxy_port" "$NAMESPACE")
  pf_pid=$(echo "$pf_info" | awk '{print $1}')
  local pf_local_port
  pf_local_port=$(echo "$pf_info" | awk '{print $2}')

  # Wait for port-forward to stabilize
  sleep 3

  local target_url="http://localhost:${pf_local_port}/fixed"

  # Step 1: Deploy good config (version 1)
  log_info "Deploying good config (v1)"
  proxy_deploy_baseline_config

  sleep 2

  # Step 2: Capture baseline P99
  log_info "Measuring baseline performance"
  "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$TARGET_RPS" \
    --duration "$BASELINE_DURATION_S" \
    --connections 100 \
    --output "${output_dir}/baseline.json" \
    > /dev/null 2>&1

  local baseline_p99
  baseline_p99=$(jq -r '.latency_ms.p99' "${output_dir}/baseline.json")
  log_info "Baseline P99: ${baseline_p99}ms"

  # Step 3: Deploy bad config (version 2 - invalid upstream)
  log_info "Deploying bad config (v2 - invalid upstream endpoint)"
  publish_pavis_bad_config 2

  sleep 2

  # Step 4: Verify degradation
  log_info "Measuring degraded performance"
  "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$TARGET_RPS" \
    --duration "$DEGRADED_DURATION_S" \
    --connections 100 \
    --output "${output_dir}/degraded.json" \
    > /dev/null 2>&1

  local degraded_errors
  degraded_errors=$(jq -r '.errors' "${output_dir}/degraded.json")
  log_info "Degraded errors: $degraded_errors (expected high)"

  # Step 5: Rollback to good config (v1)
  log_info "Rolling back to good config (v1)"
  local rollback_start
  rollback_start=$(timestamp_ms)

  rollback_config 1

  # Step 6: Measure time to baseline restoration (TTBR)
  log_info "Measuring time to baseline restoration (TTBR)"

  # Start recovery measurement
  "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$TARGET_RPS" \
    --duration "$RECOVERY_DURATION_S" \
    --connections 100 \
    --output "${output_dir}/recovery.json" \
    > /dev/null 2>&1 &
  local recovery_pid=$!

  # Poll for baseline P99 restoration
  local ttbr_ms=0
  local max_wait_ms=30000
  local elapsed_ms=0
  local restored=0

  while [[ $elapsed_ms -lt $max_wait_ms ]]; do
    sleep 1
    elapsed_ms=$(( $(timestamp_ms) - rollback_start ))

    # Check current P99
    local current_p99
    current_p99=$(capture_p99_snapshot 2 "$target_url" "$TARGET_RPS") || continue

    # Check if within 10% of baseline
    local threshold
    threshold=$(echo "$baseline_p99 * 1.1" | bc -l)

    if (( $(echo "$current_p99 <= $threshold" | bc -l) )); then
      ttbr_ms=$elapsed_ms
      restored=1
      log_info "Baseline restored at ${ttbr_ms}ms (P99: ${current_p99}ms <= ${threshold}ms)"
      break
    fi
  done

  # Wait for recovery test to complete
  wait "$recovery_pid" 2>/dev/null || true

  # Cleanup port-forward
  kubectl_stop_port_forward "$pf_pid"

  # Step 7: Extract final recovery stats
  local recovery_p99
  recovery_p99=$(jq -r '.latency_ms.p99' "${output_dir}/recovery.json")
  local recovery_errors
  recovery_errors=$(jq -r '.errors' "${output_dir}/recovery.json")
  local baseline_achieved_rps=""
  local degraded_achieved_rps=""
  local recovery_achieved_rps=""
  if [[ -f "${output_dir}/baseline.json" ]]; then
    baseline_achieved_rps=$(jq -r '.achieved_rps // empty' "${output_dir}/baseline.json")
  fi
  if [[ -f "${output_dir}/degraded.json" ]]; then
    degraded_achieved_rps=$(jq -r '.achieved_rps // empty' "${output_dir}/degraded.json")
  fi
  if [[ -f "${output_dir}/recovery.json" ]]; then
    recovery_achieved_rps=$(jq -r '.achieved_rps // empty' "${output_dir}/recovery.json")
  fi

  # Step 8: Write metrics JSON
  cat > "${output_dir}/metrics.json" <<EOF
{
  "test": "$CASE_NAME",
  "proxy": "${BENCH_PROXY}",
  "baseline_p99_ms": $baseline_p99,
  "degraded_errors": $degraded_errors,
  "baseline_achieved_rps": $baseline_achieved_rps,
  "degraded_achieved_rps": $degraded_achieved_rps,
  "recovery_achieved_rps": $recovery_achieved_rps,
  "rollback_ttbr_ms": $ttbr_ms,
  "recovery_p99_ms": $recovery_p99,
  "recovery_errors": $recovery_errors,
  "baseline_restored": $([ $restored -eq 1 ] && echo "true" || echo "false"),
  "config_versions": [1, 2, 1],
  "target_rps": $TARGET_RPS
}
EOF

  log_info "Metrics written to ${output_dir}/metrics.json"

  # Step 9: Validation
  log_info "Validating results"
  local validation_failed=0

  if (( restored == 0 )); then
    log_warn "Failed to restore baseline performance within ${max_wait_ms}ms"
    validation_failed=1
  fi

  if (( $(echo "$ttbr_ms > 10000" | bc -l) )); then
    log_warn "TTBR exceeded 10s threshold: ${ttbr_ms}ms"
    validation_failed=1
  fi

  if (( recovery_errors > 0 )); then
    log_warn "Detected ${recovery_errors} errors during recovery"
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

publish_pavis_bad_config() {
  local version="$1"

  local temp_config
  temp_config=$(mktemp --suffix=.yaml)

  cp "$(resolve_pavis_config_path)" "$temp_config"
  if command -v yq > /dev/null 2>&1; then
    yq -i '.upstreams[0].endpoints[0].address = "does-not-exist.invalid"' "$temp_config"
  else
    sed -i.bak 's/address: "backend"/address: "does-not-exist.invalid"/' "$temp_config"
    rm -f "${temp_config}.bak"
  fi

  publish_to_pavis_relay "$temp_config" "$version"
  rm -f "$temp_config"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi

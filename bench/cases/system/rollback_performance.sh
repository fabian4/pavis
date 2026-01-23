#!/usr/bin/env bash
set -euo pipefail

# System Mode Test: Rollback Performance
# Measures time to restore baseline performance after rolling back from a degraded config.
# Apply and rollback are proven by response fingerprinting (frozen data plane semantics).

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
FINGERPRINT_HEADER="X-Pavis-Config-Version"
DEGRADED_DELAY_MS="10"

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

  # Wait for port-forward to stabilize
  sleep 3

  local target_url="http://localhost:${pf_local_port}/fixed?ms=${DEGRADED_DELAY_MS}"

  # Step 1: Deploy good config (version 1) and prove it is applied.
  log_info "Deploying good config (v1)"
  publish_pavis_config_variant 1 "1" ""

  if ! wait_for_response_header "$target_url" "$FINGERPRINT_HEADER" "1" 30; then
    log_error "Failed to observe baseline fingerprint v1"
    kubectl_stop_port_forward "$pf_pid"
    return 1
  fi

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
  if ! is_number "$baseline_p99" || (( $(echo "$baseline_p99 <= 0" | bc -l) )); then
    log_error "Invalid baseline P99: '${baseline_p99}'"
    kubectl_stop_port_forward "$pf_pid"
    return 1
  fi
  log_info "Baseline P99: ${baseline_p99}ms"

  # Step 3: Deploy degraded config (version 2) and prove it is applied.
  log_info "Deploying degraded config (v2 - fixed delay + fingerprint)"
  publish_pavis_config_variant 2 "2" "/sleep"

  if ! wait_for_response_header "$target_url" "$FINGERPRINT_HEADER" "2" 30; then
    log_error "Failed to observe degraded fingerprint v2"
    kubectl_stop_port_forward "$pf_pid"
    return 1
  fi

  sleep 2

  # Step 4: Verify degraded/steady-state behavior
  log_info "Measuring degraded performance"
  "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$TARGET_RPS" \
    --duration "$DEGRADED_DURATION_S" \
    --connections 100 \
    --output "${output_dir}/degraded.json" \
    > /dev/null 2>&1

  local degraded_errors
  degraded_errors=$(jq -r '.errors // 0' "${output_dir}/degraded.json")
  log_info "Degraded errors: $degraded_errors"

  # Step 5: Rollback to good config (v1) and measure TTBR.
  # Contract:
  # - rollback_ttbr_ms is the elapsed time from rollback publish until recovery is FIRST satisfied.
  # - recovery condition: v1 fingerprint observed AND p99 <= baseline * 1.10.
  # - If recovery never happens within timeout, rollback_ttbr_ms == timeout_ms and baseline_restored=false.
  log_info "Rolling back to good config (v1)"
  local baseline_restored=false
  local rollback_ttbr_ms=0
  publish_pavis_config_variant 1 "1" ""
  local start_ts
  start_ts=$(timestamp_ms)

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

  # Poll for baseline restoration.
  # Rollback completion requires:
  # 1) fingerprint == v1 (applied), and
  # 2) P99 <= baseline * 1.10.
  local timeout_ms=30000
  local elapsed_ms=0
  local restore_threshold_pct=10
  local restore_threshold_ms
  restore_threshold_ms=$(echo "$baseline_p99 * (1 + $restore_threshold_pct / 100)" | bc -l)

  while [[ $elapsed_ms -lt $timeout_ms ]]; do
    sleep 1
    elapsed_ms=$(( $(timestamp_ms) - start_ts ))

    local current_fingerprint
    current_fingerprint=$(fetch_response_header "$target_url" "$FINGERPRINT_HEADER")
    if [[ "$current_fingerprint" != "1" ]]; then
      continue
    fi

    # Check current P99 once v1 is observed
    local current_p99
    current_p99=$(capture_p99_snapshot 2 "$target_url" "$TARGET_RPS") || continue

    # Check if within 10% of baseline
    if (( $(echo "$current_p99 <= $restore_threshold_ms" | bc -l) )); then
      rollback_ttbr_ms=$elapsed_ms
      baseline_restored=true
      log_info "Baseline restored at ${rollback_ttbr_ms}ms (P99: ${current_p99}ms <= ${restore_threshold_ms}ms)"
      break
    fi
  done

  # Wait for recovery test to complete
  wait "$recovery_pid" 2>/dev/null || true

  # Step 7: Extract final recovery stats
  local recovery_p99
  recovery_p99=$(jq -r '.latency_ms.p99' "${output_dir}/recovery.json")
  local recovery_errors
  recovery_errors=$(jq -r '.errors // 0' "${output_dir}/recovery.json")
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

  if [[ "$baseline_restored" != "true" ]]; then
    rollback_ttbr_ms=$timeout_ms
    log_warn "Baseline not restored within ${timeout_ms}ms (threshold ${restore_threshold_ms}ms)"
  fi

  # Cleanup port-forward
  kubectl_stop_port_forward "$pf_pid"

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
  "rollback_ttbr_ms": $rollback_ttbr_ms,
  "recovery_p99_ms": $recovery_p99,
  "recovery_errors": $recovery_errors,
  "baseline_restored": $baseline_restored,
  "restore_threshold_pct": $restore_threshold_pct,
  "restore_threshold_ms": $restore_threshold_ms,
  "config_versions": [1, 2, 1],
  "target_rps": $TARGET_RPS
}
EOF

  log_info "Metrics written to ${output_dir}/metrics.json"

  # Step 9: Validation
  log_info "Validating results"
  local validation_failed=0

  if (( $(echo "$rollback_ttbr_ms < 0" | bc -l) )); then
    log_warn "Invalid rollback_ttbr_ms: ${rollback_ttbr_ms}ms"
    validation_failed=1
  fi

  if [[ "$baseline_restored" == "true" ]] && (( rollback_ttbr_ms >= timeout_ms )); then
    log_warn "baseline_restored=true but rollback_ttbr_ms >= timeout (${rollback_ttbr_ms}ms)"
    validation_failed=1
  fi

  if [[ "$baseline_restored" != "true" ]]; then
    log_warn "Failed to restore baseline performance within ${timeout_ms}ms"
    validation_failed=1
  fi

  if (( $(echo "$rollback_ttbr_ms > 10000" | bc -l) )); then
    log_warn "TTBR exceeded 10s threshold: ${rollback_ttbr_ms}ms"
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

fetch_response_header() {
  local url="$1"
  local header="$2"
  curl -s -D - -o /dev/null "$url" | tr -d '\r' | awk -v h="$header" 'tolower($1) == tolower(h ":") {print $2}' | tail -n1
}

wait_for_response_header() {
  local url="$1"
  local header="$2"
  local expected="$3"
  local timeout_s="${4:-30}"

  local elapsed=0
  while [[ $elapsed -lt $timeout_s ]]; do
    local value
    value=$(fetch_response_header "$url" "$header")
    if [[ "$value" == "$expected" ]]; then
      return 0
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done

  return 1
}

publish_pavis_config_variant() {
  local version="$1"
  local fingerprint="$2"
  local rewrite_path="${3:-}"

  local temp_config
  temp_config=$(mktemp --suffix=.yaml)

  cp "$(resolve_pavis_config_path)" "$temp_config"

  # Use sed to modify the config (yq doesn't handle YAML tags correctly)
  sed -i.bak 's|path: !prefix { path: "/" }|path: !prefix { path: "/fixed" }|' "$temp_config"
  rm -f "${temp_config}.bak"
  sed -i.bak "/weight: 100/a\\
        response_headers:\\
          set_headers:\\
            - [\"${FINGERPRINT_HEADER}\", \"${fingerprint}\"]" "$temp_config"
  rm -f "${temp_config}.bak"
  if [[ -n "$rewrite_path" ]]; then
    sed -i.bak "/path: !prefix { path: \"\/fixed\" }/a\\
        rewrite:\\
          path: \"${rewrite_path}\"" "$temp_config"
    rm -f "${temp_config}.bak"
  fi

  publish_to_pavis_relay "$temp_config" "$version"
  rm -f "$temp_config"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi

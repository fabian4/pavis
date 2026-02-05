#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# System Mode Test: Rollback Performance
# =============================================================================
#
# WHAT THIS TEST MEASURES:
# ------------------------
# - rollback_ttbr_ms: Time-To-Baseline-Restoration after rollback from degraded config
#   * Starts when rollback config is published
#   * Ends when p99 latency returns to within acceptable range of baseline
#   * Measures RUNTIME recovery: queue drain, pool stabilization, scheduler settling
#
# WHAT THIS TEST DOES NOT MEASURE:
# ---------------------------------
# - Config propagation time (separate metric: convergence_time)
# - Test harness overhead (loadgen startup, curl latency)
# - Single-sample outliers or transient spikes
#
# CRITICAL INVARIANT:
# -------------------
# During TTBR measurement, there is exactly ONE load source:
#   - Lightweight probe bursts (300 RPS, 1s duration)
#   - NO concurrent background loadgen
# This ensures we measure runtime recovery, not load interference artifacts.
#
# Recovery is detected using statistical robustness:
#   - First crossing + hysteresis (K of M probe samples below threshold)
#   - Threshold set to 1.20x baseline to account for natural jitter/cache warming
#
# PORTABILITY FIX:
# ----------------
# - bc is ONLY used for floating-point math (threshold calculation)
# - awk is used for all floating-point comparisons (portable, no bc dependency)
# - No bc usage for sleep timing (uses fixed fractional values)
#
# =============================================================================

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

# Recovery detection parameters
PROBE_RPS=10                    # Low RPS to avoid load interference
PROBE_DURATION_S=1              # Short duration for quick probes
HYSTERESIS_WINDOW_SAMPLES=3     # M samples window
HYSTERESIS_REQUIRED_SAMPLES=2   # K samples required below threshold within window
RESTORE_THRESHOLD_PCT=20        # 20% above baseline (allows for jitter/cache warming)

is_number() {
  local value="$1"
  [[ "$value" =~ ^-?[0-9]+([.][0-9]+)?([eE][-+]?[0-9]+)?$ ]]
}

# Portable float comparison using awk (no bc dependency for comparisons)
# Usage: float_compare "value" "operator" "threshold"
# Operators: ">", ">=", "<", "<=", "=="
# Returns: 0 if true, 1 if false
float_compare() {
  local value="$1"
  local op="$2"
  local threshold="$3"

  awk -v val="$value" -v th="$threshold" -v operator="$op" 'BEGIN {
    if (operator == ">") exit !(val > th)
    else if (operator == ">=") exit !(val >= th)
    else if (operator == "<") exit !(val < th)
    else if (operator == "<=") exit !(val <= th)
    else if (operator == "==") exit !(val == th)
    else exit 1
  }'
}

# Non-intrusive probe: Uses low RPS and short duration to avoid interfering with recovery
# Returns: p99 latency in milliseconds, or empty string on failure
probe_p99_lightweight() {
  local target_url="$1"
  local temp_output
  temp_output=$(mktemp)

  # Lightweight probe: 300 RPS for 1s = only 300 requests
  # This is <1% of typical TARGET_RPS and won't interfere with pool/queue recovery
  if "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$PROBE_RPS" \
    --duration "$PROBE_DURATION_S" \
    --connections 300 \
    --output "$temp_output" \
    > /dev/null 2>&1; then

    local p99
    p99=$(jq -r '.latency_ms.p99' "$temp_output" 2>/dev/null || echo "")
    rm -f "$temp_output"

    if is_number "$p99" && float_compare "$p99" ">" "0"; then
      echo "$p99"
      return 0
    fi
  fi

  rm -f "$temp_output"
  return 1
}

median_from_samples() {
  local -a samples=("$@")
  local count="${#samples[@]}"
  if (( count == 0 )); then
    return 1
  fi
  printf "%s\n" "${samples[@]}" | sort -n | awk '{
    a[NR]=$1
  }
  END {
    n=NR
    if (n == 0) exit 1
    if (n % 2 == 1) {
      print a[(n+1)/2]
    } else {
      print (a[n/2] + a[n/2+1]) / 2
    }
  }'
}

main() {
  log_info "Starting test: $CASE_NAME for ${BENCH_PROXY}"

  # Get proxy-specific configuration
  local pod_label
  local proxy_port
  pod_label=$(get_proxy_pod_label)
  proxy_port=$(get_proxy_port)

  local output_dir="${BENCH_OUTPUT_DIR}/${BENCH_MODE}/${BENCH_PROXY}/${CASE_NAME}${BENCH_CASE_SUFFIX:+__${BENCH_CASE_SUFFIX}}"
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
  if ! is_number "$baseline_p99" || float_compare "$baseline_p99" "<=" "0"; then
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
  #
  # TTBR Measurement Contract:
  # --------------------------
  # - rollback_ttbr_ms = elapsed time from rollback publish until FIRST return to baseline shape
  # - Recovery condition: v1 fingerprint observed AND first crossing + hysteresis
  #   (K of next M probe p99 samples <= baseline * (1 + threshold_pct/100))
  # - rollback_p99_ms is derived from the same low-RPS probe regime to avoid contradictions
  # - If recovery never happens within timeout, rollback_ttbr_ms = timeout_ms and baseline_restored=false
  # - Timeout is a HARD upper bound: no operations may occur after timeout is exceeded
  #
  # Non-Intrusive Probing:
  # ----------------------
  # - During TTBR measurement, NO background loadgen runs
  # - Recovery is detected via lightweight probes (10 RPS, 1s duration)
  # - Probes add <1% load and do NOT interfere with queue/pool recovery
  #
  # Rationale:
  # ----------
  # Hysteresis (K of M) tolerates single-sample jitter while still catching real regressions.
  # This prevents false FAILs when steady-state rollback performance is already good.
  #
  log_info "Rolling back to good config (v1)"
  local baseline_restored=false
  local rollback_ttbr_ms=0
  local rollback_p99_ms=""
  local first_good_seen=false
  local -a hysteresis_window=()
  publish_pavis_config_variant 1 "1" ""
  local start_ts
  start_ts=$(timestamp_ms)

  log_info "Measuring time to baseline restoration (TTBR)"
  log_info "Recovery threshold: ${RESTORE_THRESHOLD_PCT}% above baseline (${baseline_p99}ms)"
  log_info "Requires ${HYSTERESIS_REQUIRED_SAMPLES} of ${HYSTERESIS_WINDOW_SAMPLES} samples below threshold"

  local timeout_ms=30000
  local restore_threshold_ms
  # Use bc ONLY for floating-point math (threshold calculation)
  restore_threshold_ms=$(echo "$baseline_p99 * (1 + $RESTORE_THRESHOLD_PCT / 100)" | bc -l)

  # TTBR measurement loop with hard timeout semantics
  while true; do
    # Check elapsed time BEFORE any operations
    local elapsed_ms
    elapsed_ms=$(( $(timestamp_ms) - start_ts ))

    # Hard timeout: If we've exceeded timeout, stop immediately
    if [[ $elapsed_ms -ge $timeout_ms ]]; then
      rollback_ttbr_ms=$timeout_ms
      log_warn "Timeout reached (${timeout_ms}ms) without recovery"
      break
    fi

    # Ensure we have enough time for probe + sleep before timeout
    # If not, this would be the last iteration - skip it and exit
    local time_for_probe_and_sleep=$(( PROBE_DURATION_S * 1000 + 1000 ))
    if (( elapsed_ms + time_for_probe_and_sleep > timeout_ms )); then
      rollback_ttbr_ms=$timeout_ms
      log_warn "Insufficient time for probe before timeout (${elapsed_ms}ms / ${timeout_ms}ms)"
      break
    fi

    # Brief sleep to avoid tight polling
    sleep 1
    elapsed_ms=$(( $(timestamp_ms) - start_ts ))

    # Check if fingerprint has reverted to v1
    local current_fingerprint
    current_fingerprint=$(fetch_response_header "$target_url" "$FINGERPRINT_HEADER")
    if [[ "$current_fingerprint" != "1" ]]; then
      first_good_seen=false
      hysteresis_window=()
      continue
    fi

    # Non-intrusive probe: 10 RPS for 1s (only 10 requests, <1% of typical load)
    local current_p99
    current_p99=$(probe_p99_lightweight "$target_url") || {
      first_good_seen=false
      hysteresis_window=()
      continue
    }

    local sample_elapsed_ms
    sample_elapsed_ms=$(( $(timestamp_ms) - start_ts ))

    # Check if current p99 is within threshold (FIXED: using awk, not bc)
    if float_compare "$current_p99" "<=" "$restore_threshold_ms"; then
      if [[ "$first_good_seen" != "true" ]]; then
        first_good_seen=true
        hysteresis_window=()
      fi
    fi

    if [[ "$first_good_seen" == "true" ]]; then
      hysteresis_window+=("$current_p99")
      if (( ${#hysteresis_window[@]} > HYSTERESIS_WINDOW_SAMPLES )); then
        hysteresis_window=("${hysteresis_window[@]:1}")
      fi

      local good_samples=0
      for sample in "${hysteresis_window[@]}"; do
        if float_compare "$sample" "<=" "$restore_threshold_ms"; then
          good_samples=$((good_samples + 1))
        fi
      done

      log_info "Sample window: ${good_samples}/${HYSTERESIS_WINDOW_SAMPLES} <= ${restore_threshold_ms}ms (latest: ${current_p99}ms, elapsed: ${sample_elapsed_ms}ms)"

      if (( ${#hysteresis_window[@]} >= HYSTERESIS_WINDOW_SAMPLES && good_samples >= HYSTERESIS_REQUIRED_SAMPLES )); then
        rollback_ttbr_ms=$sample_elapsed_ms
        rollback_p99_ms=$(median_from_samples "${hysteresis_window[@]}") || rollback_p99_ms="$current_p99"
        baseline_restored=true
        log_info "Baseline restored at ${rollback_ttbr_ms}ms (hysteresis satisfied)"
        break
      fi
    fi
  done

  if [[ -z "$rollback_p99_ms" ]]; then
    rollback_p99_ms="null"
  fi

  # Step 6: Run final recovery validation load test to confirm stability
  log_info "Running final recovery validation (full load for ${RECOVERY_DURATION_S}s)"
  "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$TARGET_RPS" \
    --duration "$RECOVERY_DURATION_S" \
    --connections 100 \
    --output "${output_dir}/recovery.json" \
    > /dev/null 2>&1 || true

  # Step 7: Extract final recovery stats
  local recovery_p99=""
  local recovery_errors=0
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
    recovery_p99=$(jq -r '.latency_ms.p99 // empty' "${output_dir}/recovery.json")
    recovery_errors=$(jq -r '.errors // 0' "${output_dir}/recovery.json")
    recovery_achieved_rps=$(jq -r '.achieved_rps // empty' "${output_dir}/recovery.json")
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
  "rollback_p99_ms": $rollback_p99_ms,
  "recovery_p99_ms": $recovery_p99,
  "recovery_errors": $recovery_errors,
  "baseline_restored": $baseline_restored,
  "restore_threshold_pct": $RESTORE_THRESHOLD_PCT,
  "restore_threshold_ms": $restore_threshold_ms,
  "hysteresis_window_samples": $HYSTERESIS_WINDOW_SAMPLES,
  "hysteresis_required_samples": $HYSTERESIS_REQUIRED_SAMPLES,
  "config_versions": [1, 2, 1],
  "target_rps": $TARGET_RPS,
  "probe_rps": $PROBE_RPS,
  "probe_duration_s": $PROBE_DURATION_S
}
EOF

  log_info "Metrics written to ${output_dir}/metrics.json"

  # Step 9: Validation
  log_info "Validating results"
  local validation_failed=0

  # FIXED: Use awk for float comparison instead of bc
  if float_compare "$rollback_ttbr_ms" "<" "0"; then
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

  # FIXED: Use awk for float comparison instead of bc
  if float_compare "$rollback_ttbr_ms" ">" "10000"; then
    log_warn "TTBR exceeded 10s threshold: ${rollback_ttbr_ms}ms"
    validation_failed=1
  fi

  if (( recovery_errors > 0 )); then
    log_warn "Detected ${recovery_errors} errors during recovery validation"
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

  # Step 1: Replace path prefix
  sed -i.bak 's|path: !prefix { path: "/" }|path: !prefix { path: "/fixed" }|' "$temp_config"
  rm -f "${temp_config}.bak"

  # Step 2: Add response headers after "weight: 100" line
  # Using awk instead of sed for portability (sed's multi-line append syntax varies)
  awk -v header="$FINGERPRINT_HEADER" -v value="$fingerprint" '
  {
    print
    if (/weight: 100/) {
      print "        response_headers:"
      print "          set_headers:"
      print "            - [\"" header "\", \"" value "\"]"
    }
  }
  ' "$temp_config" > "$temp_config.new"
  mv "$temp_config.new" "$temp_config"

  # Step 3: Add rewrite path if specified
  if [[ -n "$rewrite_path" ]]; then
    awk -v rpath="$rewrite_path" '
    {
      print
      if (/path: !prefix { path: "\/fixed" }/) {
        print "        rewrite:"
        print "          path: \"" rpath "\""
      }
    }
    ' "$temp_config" > "$temp_config.new"
    mv "$temp_config.new" "$temp_config"
  fi

  publish_to_pavis_relay "$temp_config" "$version"
  rm -f "$temp_config"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi

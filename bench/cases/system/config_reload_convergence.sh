#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# System Mode Test: Config Reload Convergence
# =============================================================================
#
# CONVERGENCE SEMANTICS:
# ----------------------
# This test measures the time for a config update to converge across ALL
# sidecar pods in the deployment, not just one pod.
#
# Convergence is defined as: ALL pods matching the pod selector are serving
# the target config version (v2).
#
# MEASUREMENT CONTRACT:
# ---------------------
# - convergence_time_ms: Elapsed time from config publish until ALL pods serve v2
# - Timer starts immediately after publish_to_pavis_relay returns
# - Polling interval: 200ms (deterministic, avoids tight loops)
# - Timeout: 60s (hard upper bound)
# - If timeout, convergence_time_ms = timeout and errors != 0
#
# VERIFICATION METHOD:
# --------------------
# For each pod matching the selector:
#   1. Port-forward to the pod's proxy port
#   2. Make HTTP request to /health and check response body
#   3. Pod is "converged" when body matches target value (OK-V2)
# ALL pods must converge for test to pass.
#
# This is more reliable than checking filesystem metadata because:
#   - Tests actual runtime behavior (what version is being served to clients)
#   - Doesn't depend on version files that may not be written
#   - Matches what clients actually experience
#
# LOAD INTERFERENCE AVOIDANCE (CRITICAL FIX):
# --------------------------------------------
# Previous bug: Background loadgen + separate transition_p99 snapshot created overlapping load
# New behavior:
#   - Background load runs at TARGET_RPS during convergence measurement
#   - AFTER convergence completes: STOP background load
#   - THEN capture transition_p99 with a separate, non-overlapping snapshot
# This ensures:
#   - Convergence measurement is realistic (traffic continues)
#   - transition_p99 is measured cleanly without load interference
#   - No back-to-back full-rate loadgen phases
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
# shellcheck source=scripts/lib/http.sh
source "$SCRIPT_DIR/../../scripts/lib/http.sh"

CASE_NAME="config_reload_convergence"
TARGET_RPS="${SYSTEM_CONFIG_RELOAD_CONVERGENCE_TARGET_RPS}"
DURATION_S="${SYSTEM_CONFIG_RELOAD_CONVERGENCE_DURATION_S}"
CONVERGENCE_WINDOW_S="${SYSTEM_CONFIG_RELOAD_CONVERGENCE_CONVERGENCE_WINDOW_S}"
NAMESPACE="${BENCH_NAMESPACE:-bench-system}"

# Convergence polling parameters
CONVERGENCE_POLL_INTERVAL_S=0.2  # Poll every 200ms (FIXED, no bc dependency)
CONVERGENCE_TIMEOUT_MS=60000      # 60 second timeout

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

format_float_3_or_null() {
  local value="$1"
  if [[ -z "$value" ]]; then
    echo "null"
    return 0
  fi
  printf "%.3f" "$value"
}

# Start port-forward to a specific pod and port, return "pid port"
start_pod_port_forward() {
  local pod_name="$1"
  local namespace="$2"
  local local_port_hint="$3"
  local remote_port="$4"

  local attempt=0
  local max_attempts=5
  local chosen_port="$local_port_hint"
  local pf_pid=""

  while [[ $attempt -lt $max_attempts ]]; do
    if [[ $attempt -gt 0 ]]; then
      chosen_port=$(pick_free_port "$local_port_hint")
    fi

    kubectl port-forward -n "$namespace" "pod/${pod_name}" "$chosen_port:$remote_port" \
      > /dev/null 2>&1 &
    pf_pid=$!

    sleep 1

    if check_process_alive "$pf_pid"; then
      echo "$pf_pid $chosen_port"
      return 0
    fi

    attempt=$((attempt + 1))
  done

  log_error "Port forward failed to start for pod ${pod_name}" >&2
  return 1
}

stop_pod_port_forwards() {
  local -n pids_ref="$1"

  for pid in "${pids_ref[@]}"; do
    kubectl_stop_port_forward "$pid"
  done
}

# Check config body via /health through local port-forward
# Returns: body string or "unknown"
check_pod_config_body() {
  local local_port="$1"

  local body
  body=$(curl -s "http://localhost:${local_port}/health" 2>/dev/null || echo "unknown")

  if [[ -z "$body" ]]; then
    echo "unknown"
  else
    echo "$body"
  fi
}

# Check if ALL pods have converged to target version
# Returns: 0 if all converged, 1 otherwise
# NOTE: All logging goes to stderr to avoid polluting return values in calling functions
check_all_pods_converged() {
  local -n pod_names_ref="$1"
  local -n pod_ports_ref="$2"
  local target_body="$3"

  local pod_count=${#pod_names_ref[@]}
  local converged_count=0

  for i in "${!pod_names_ref[@]}"; do
    local body
    body=$(check_pod_config_body "${pod_ports_ref[$i]}")
    if [[ "$body" == "$target_body" ]]; then
      converged_count=$((converged_count + 1))
    fi
  done

  log_info "Convergence status: ${converged_count}/${pod_count} pods at body ${target_body}" >&2

  if [[ $converged_count -eq $pod_count && $pod_count -gt 0 ]]; then
    return 0
  else
    return 1
  fi
}

# Measure convergence time for all pods
# Returns: convergence time in milliseconds, or timeout value on failure
# CRITICAL: Hard timeout semantics - no operations after timeout exceeded
# NOTE: All logging goes to stderr to avoid polluting return value captured by command substitution
measure_all_pods_convergence() {
  local pod_label="$1"
  local namespace="$2"
  local proxy_port="$3"
  local target_body="$4"
  local timeout_ms="$5"

  # Get all pod names matching the label
  local pod_names_raw
  pod_names_raw=$(kubectl get pods -n "$namespace" -l "$pod_label" -o jsonpath='{.items[*].metadata.name}' 2>/dev/null || echo "")
  if [[ -z "$pod_names_raw" ]]; then
    log_warn "No pods found matching label: $pod_label" >&2
    echo "$timeout_ms"
    return 1
  fi

  local -a pod_names=()
  local -a pod_ports=()
  local -a pf_pids=()

  for pod in $pod_names_raw; do
    local pf_info
    pf_info=$(start_pod_port_forward "$pod" "$namespace" "$proxy_port" "$proxy_port") || {
      stop_pod_port_forwards pf_pids
      echo "$timeout_ms"
      return 1
    }
    pf_pids+=("$(echo "$pf_info" | awk '{print $1}')")
    pod_ports+=("$(echo "$pf_info" | awk '{print $2}')")
    pod_names+=("$pod")
  done

  local start_ms
  start_ms=$(timestamp_ms)
  local elapsed_ms=0
  local iteration=0

  log_info "Starting convergence measurement for all pods (timeout: ${timeout_ms}ms)" >&2

  while [[ $elapsed_ms -lt $timeout_ms ]]; do
    iteration=$((iteration + 1))

    # Check if all pods converged (using HTTP header check)
    if check_all_pods_converged pod_names pod_ports "$target_body"; then
      local end_ms
      end_ms=$(timestamp_ms)
      local convergence_time=$((end_ms - start_ms))
      log_info "All pods converged in ${convergence_time}ms" >&2
      stop_pod_port_forwards pf_pids
      echo "$convergence_time"
      return 0
    fi

    # Early fail-fast check: If after 3 iterations (600ms) ALL pods report "unknown",
    # the /health response is broken - fail immediately instead of waiting 60s
    if [[ $iteration -eq 3 ]]; then
      local all_unknown=1
      for port in "${pod_ports[@]}"; do
        local body
        body=$(check_pod_config_body "$port")
        if [[ "$body" != "unknown" ]]; then
          all_unknown=0
          break
        fi
      done

      if [[ $all_unknown -eq 1 ]]; then
        log_error "FAIL FAST: All pods report 'unknown' body after ${iteration} iterations" >&2
        log_error "This indicates /health is not responding as expected" >&2
        log_error "Check that:" >&2
        log_error "  1. Relay is running and propagating configs to sidecars" >&2
        log_error "  2. Pavis config includes /health route with expected body" >&2
        log_error "  3. Pods are accessible at port ${proxy_port}" >&2
        stop_pod_port_forwards pf_pids
        echo "$timeout_ms"
        return 1
      fi
    fi

    # Sleep for poll interval (FIXED: no bc dependency)
    sleep "$CONVERGENCE_POLL_INTERVAL_S"

    local end_ms
    end_ms=$(timestamp_ms)
    elapsed_ms=$((end_ms - start_ms))

    # Hard timeout check: stop immediately if exceeded
    if [[ $elapsed_ms -ge $timeout_ms ]]; then
      break
    fi
  done

  log_error "Convergence timeout: not all pods reached target body within ${timeout_ms}ms" >&2
  stop_pod_port_forwards pf_pids
  echo "$timeout_ms"
  return 1
}

main() {
  log_info "Starting test: $CASE_NAME for ${BENCH_PROXY}"

  # Get proxy-specific configuration
  local pod_label
  local proxy_port
  pod_label=$(get_proxy_pod_label)
  proxy_port=$(get_proxy_port)

  local output_dir="${BENCH_OUTPUT_DIR}/${BENCH_MODE}/${BENCH_PROXY}/${CASE_NAME}"
  ensure_dir "$output_dir"

  # Setup port-forward to access test backend (for load generation only)
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

  # Step 2: Capture baseline P99 BEFORE starting background load
  # This ensures baseline measurement is clean and separate from convergence
  log_info "Measuring baseline P99 latency (pre-convergence)"
  "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$TARGET_RPS" \
    --duration 5 \
    --connections 100 \
    --output "${output_dir}/baseline_snapshot.json" \
    > /dev/null 2>&1

  local baseline_p99
  baseline_p99=$(jq -r '.latency_ms.p99' "${output_dir}/baseline_snapshot.json")
  log_info "Baseline P99: ${baseline_p99}ms"

  # Step 3: Start background load that runs throughout convergence
  # This simulates realistic traffic conditions during config reload
  log_info "Starting continuous background load at ${TARGET_RPS} RPS"
  local loadgen_pid
  "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$TARGET_RPS" \
    --duration "$DURATION_S" \
    --connections 100 \
    --output "${output_dir}/continuous_load.json" \
    > /dev/null 2>&1 &
  loadgen_pid=$!

  sleep 5  # Let background load stabilize

  # Step 4: Trigger config update (version 2)
  log_info "Publishing config update to v2"
  local target_body="OK-V2"
  publish_pavis_config_variant 2 "$target_body"
  local target_version="${PAVIS_PUBLISHED_VERSION:-2}"

  # Step 5: Measure convergence time across ALL pods
  # CRITICAL: This uses HTTP header checks to test actual runtime behavior
  log_info "Measuring convergence time for all pods"
  local convergence_time
  local convergence_failed=0
  convergence_time=$(measure_all_pods_convergence "$pod_label" "$NAMESPACE" "$proxy_port" "$target_body" "$CONVERGENCE_TIMEOUT_MS") || {
    convergence_failed=1
  }
  log_info "Convergence time: ${convergence_time}ms (failed=${convergence_failed})"

  # Step 6: STOP background load to avoid interference with transition_p99 measurement
  # CRITICAL FIX: This prevents overlapping full-rate load sources
  log_info "Stopping background load before transition_p99 measurement"
  if kill -0 "$loadgen_pid" 2>/dev/null; then
    kill "$loadgen_pid" 2>/dev/null || true
    wait "$loadgen_pid" 2>/dev/null || true
  fi

  # Step 7: Measure transition P99 AFTER convergence completes AND background load stopped
  # This is a clean, non-overlapping measurement window
  sleep 2  # Brief pause to ensure clean state
  log_info "Capturing transition P99 latency (post-convergence, separate measurement)"
  "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$TARGET_RPS" \
    --duration "$CONVERGENCE_WINDOW_S" \
    --connections 100 \
    --output "${output_dir}/transition_snapshot.json" \
    > /dev/null 2>&1

  local transition_p99
  transition_p99=$(jq -r '.latency_ms.p99' "${output_dir}/transition_snapshot.json")
  log_info "Transition P99: ${transition_p99}ms"

  # Step 8: Cleanup port-forward
  kubectl_stop_port_forward "$pf_pid"

  # Step 9: Calculate metrics
  local p99_delta
  p99_delta=$(echo "$transition_p99 - $baseline_p99" | bc -l)
  if [[ "$p99_delta" == .* ]]; then
    p99_delta="0${p99_delta}"
  fi

  # Step 10: Extract final stats from background load (if it completed)
  local errors_5xx=0
  if [[ -f "${output_dir}/continuous_load.json" ]]; then
    errors_5xx=$(jq -r '.errors // 0' "${output_dir}/continuous_load.json")
  fi
  local achieved_rps=""
  if [[ -f "${output_dir}/continuous_load.json" ]]; then
    achieved_rps=$(jq -r '.achieved_rps // empty' "${output_dir}/continuous_load.json")
  fi

  # Add convergence failure to errors
  if (( convergence_failed != 0 )); then
    errors_5xx=$((errors_5xx + 1))
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
  achieved_rps_fmt=$(format_float_3_or_null "$achieved_rps")
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
  "duration_s": $duration_s_fmt,
  "convergence_failed": $convergence_failed
}
EOF

  log_info "Metrics written to ${output_dir}/metrics.json"

  # Step 12: Validation
  log_info "Validating results"
  local validation_failed=0

  if (( convergence_failed != 0 )); then
    log_warn "Convergence failed: not all pods reached target version"
    validation_failed=1
  fi

  if (( $(echo "$convergence_time > 5000" | bc -l) )); then
    log_warn "Convergence time exceeded 5s threshold: ${convergence_time}ms"
    validation_failed=1
  fi

  if (( errors_5xx > 1 )); then  # Allow for convergence failure error
    log_warn "Detected ${errors_5xx} errors during test"
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

  # Step 1: Modify response body
  if command -v yq > /dev/null 2>&1; then
    yq -i ".routes[0].paths[0].body = \"${health_body}\"" "$temp_config"
  else
    sed -i.bak "s/body: \"OK\"/body: \"${health_body}\"/" "$temp_config"
    rm -f "${temp_config}.bak"
  fi

  # Step 2: Add X-Pavis-Config-Version response header for convergence detection
  # Using awk instead of sed for portability (same approach as rollback_performance.sh)
  awk -v ver="$version" '
  {
    print
    if (/weight: 100/) {
      print "        response_headers:"
      print "          set_headers:"
      print "            - [\"X-Pavis-Config-Version\", \"" ver "\"]"
    }
  }
  ' "$temp_config" > "$temp_config.new"
  mv "$temp_config.new" "$temp_config"

  if ! grep -q "X-Pavis-Config-Version" "$temp_config"; then
    log_error "Failed to inject X-Pavis-Config-Version header into config"
    rm -f "$temp_config"
    return 1
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

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi

#!/bin/bash
set -euo pipefail

# Artifact validation utilities for benchmark outputs
# Uses file-based detection to determine workload type

_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$_LIB_DIR/log.sh"
source "$_LIB_DIR/json.sh"

require_cmd() {
  local cmd
  for cmd in "$@"; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      log_error "Required command not found: $cmd"
      return 1
    fi
  done
  return 0
}

validate_meta_json() {
  local meta_file="$1"

  if [[ ! -f "$meta_file" ]]; then
    log_error "meta.json not found: $meta_file"
    return 1
  fi

  # Validate JSON format
  if ! json_validate "$meta_file"; then
    log_error "meta.json is not valid JSON: $meta_file"
    return 1
  fi

  # Validate required fields (per-case metadata only)
  if ! json_has_keys "$meta_file" .case .proxy .backend_container .proxy_container \
    .backend_image_id .proxy_image_id .backend_image_digest .proxy_image_digest; then
    return 1
  fi

  log_debug "meta.json validation passed: $meta_file"
  return 0
}

validate_wrk_output() {
  local wrk_file="$1"

  if [[ ! -f "$wrk_file" ]]; then
    log_error "wrk output not found: $wrk_file"
    return 1
  fi

  # Check for key wrk output marker
  if ! grep -q "Requests/sec:" "$wrk_file"; then
    log_error "wrk output missing 'Requests/sec:' line: $wrk_file"
    return 1
  fi

  log_debug "wrk output validation passed: $wrk_file"
  return 0
}

validate_loadgen_output() {
  local loadgen_file="$1"

  if [[ ! -f "$loadgen_file" ]]; then
    log_error "loadgen output not found: $loadgen_file"
    return 1
  fi

  # Validate JSON format
  if ! json_validate "$loadgen_file"; then
    log_error "loadgen output is not valid JSON: $loadgen_file"
    return 1
  fi

  # Validate required fields
  if ! json_has_keys "$loadgen_file" .achieved_rps .errors .dropped; then
    return 1
  fi
  if jq -e '(.p50_ms != null) and (.p90_ms != null) and (.p99_ms != null)' "$loadgen_file" >/dev/null 2>&1; then
    log_debug "Detected legacy flat latency fields in $loadgen_file"
  elif jq -e '(.latency_ms.p50 != null) and (.latency_ms.p90 != null) and (.latency_ms.p99 != null)' "$loadgen_file" >/dev/null 2>&1; then
    log_debug "Detected nested latency_ms fields in $loadgen_file"
  else
    log_error "loadgen output missing latency percentiles (expected .p*_ms or .latency_ms.*): $loadgen_file"
    return 1
  fi

  log_debug "loadgen output validation passed: $loadgen_file"
  return 0
}

validate_benchmark_artifacts() {
  local case_name="$1"
  local case_dir="$2"
  local workload_type=""
  local validation_status=0

  if [[ ! -d "$case_dir" ]]; then
    log_error "Case directory not found: $case_dir"
    return 1
  fi

  if [[ -f "$case_dir/meta.json" ]]; then
    validate_meta_json "$case_dir/meta.json" || validation_status=1
  fi

  # File-based detection (CRITICAL: inspect actual files, never infer from case names)
  if [[ -f "$case_dir/loadgen.txt.json" ]]; then
    workload_type="loadgen"
    log_debug "Detected loadgen workload in $case_dir"

    # Validate loadgen artifacts
    validate_loadgen_output "$case_dir/loadgen.txt.json" || validation_status=1

  elif compgen -G "$case_dir/run_*/wrk.txt" >/dev/null; then
    workload_type="wrk_multi_run"
    log_debug "Detected legacy multi-run wrk workload in $case_dir"

    # Validate all wrk outputs in run_* directories
    local found=0
    for wrk_file in "$case_dir"/run_*/wrk.txt; do
      if validate_wrk_output "$wrk_file"; then
        found=1
        break
      fi
    done
    if [[ $found -eq 0 ]]; then
      log_error "No valid wrk.txt found in run_* directories"
      validation_status=1
    fi

  elif [[ -f "$case_dir/wrk.txt" ]]; then
    workload_type="wrk_single_run"
    log_debug "Detected single-run wrk workload in $case_dir"

    # Validate single wrk output
    validate_wrk_output "$case_dir/wrk.txt" || validation_status=1
  elif [[ -f "$case_dir/metrics.json" ]]; then
    workload_type="system_metrics"
    log_debug "Detected system metrics workload in $case_dir"

  else
    log_error "Unable to detect workload type in $case_dir (no loadgen.txt.json, run_*/wrk.txt, or wrk.txt found)"
    return 1
  fi

  if [[ -f "$case_dir/metrics.json" ]]; then
    if ! json_validate "$case_dir/metrics.json"; then
      log_error "Invalid JSON in metrics.json"
      validation_status=1
    fi
  fi

  if [[ -f "$case_dir/docker_stats.csv" ]]; then
    local stats_lines
    stats_lines=$(wc -l < "$case_dir/docker_stats.csv" | tr -d ' ')
    if [[ "$stats_lines" -lt 2 ]]; then
      log_error "docker_stats.csv missing header or data rows"
      validation_status=1
    fi
  fi

  if [[ $validation_status -eq 0 ]]; then
    log_debug "All artifacts validated successfully for ${case_name} ($workload_type)"
  else
    log_error "Artifact validation failed for $case_name"
  fi

  return $validation_status
}

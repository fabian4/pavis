#!/usr/bin/env bash
set -euo pipefail

# Benchmark runner: sequentially executes case scripts and writes an index.
# Assumptions:
# - docker and docker compose are available.
# - wrk and wrk2 are installed on the host.
# - Individual case scripts under bench/cases are self-contained.
#
# Environment variables:
# - PROXY: Target proxy (pavis, envoy, nginx, haproxy). Default: pavis
# - CASE: Space-separated list of test cases. Default: all cases
# - DRY_RUN: If set to "1" or "true", validate setup without running benchmarks

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CASES_DIR="${ROOT_DIR}/bench/cases"
RESULTS_DIR="${ROOT_DIR}/bench/output"

PROXY="${PROXY:-pavis}"
CASE="${CASE:-throughput_short_1x latency_short_1x latency_extended_1x concurrency_short_1x churn_short_1x reload_short_1x}"
DRY_RUN="${DRY_RUN:-}"

# Export DRY_RUN so case scripts can access it
export DRY_RUN

# PVS config management for pavis
PVS_CONFIG="${ROOT_DIR}/bench/config/pavis.pvs"
YAML_CONFIG="${ROOT_DIR}/bench/config/pavis.yaml"
PVS_GENERATED=false

generate_pvs() {
  if [ "$PROXY" = "pavis" ]; then
    if [ -f "$PVS_CONFIG" ]; then
      echo "PVS config already exists at $PVS_CONFIG"
      return
    fi

    if [ ! -f "$YAML_CONFIG" ]; then
      echo "error: YAML config not found at $YAML_CONFIG" >&2
      exit 1
    fi

    local pavctl="${ROOT_DIR}/target/release/pavctl"
    if [ ! -x "$pavctl" ]; then
      echo "Building pavctl..."
      cargo build -p pavctl --release --quiet
    fi

    echo "Generating PVS config from $YAML_CONFIG..."
    "$pavctl" gen "$YAML_CONFIG" "$PVS_CONFIG" >/dev/null
    PVS_GENERATED=true
  fi
}

cleanup_pvs() {
  if [ "$PVS_GENERATED" = true ] && [ -f "$PVS_CONFIG" ]; then
    echo "Cleaning up generated PVS config..."
    rm -f "$PVS_CONFIG"
  fi
}

# Ensure cleanup happens on exit
trap cleanup_pvs EXIT

index_file() {
  echo "${RESULTS_DIR}/${PROXY}/index.csv"
}

run_case() {
  local case_name="$1"
  local proxy="$2"
  local script="${CASES_DIR}/${case_name}.sh"

  if [ ! -x "$script" ]; then
    echo "error: missing case script $script" >&2
    exit 1
  fi

  if [ "$DRY_RUN" = "1" ] || [ "$DRY_RUN" = "true" ]; then
    echo "=== [DRY-RUN] ${case_name} (PROXY=${proxy}) ==="
  else
    echo "=== running ${case_name} (PROXY=${proxy}) ==="
  fi
  PROXY="$proxy" "$script"
}

append_index() {
  local index="$1"
  local case_name="$2"
  local proxy="$3"
  local result_path
  local summary_path

  result_path="${RESULTS_DIR}/${PROXY}/${case_name}"
  if [ ! -d "$result_path" ]; then
    echo "warn: no result path found for ${case_name}" >&2
    return
  fi

  summary_path="${result_path}/summary.json"
  if [ ! -f "$summary_path" ]; then
    echo "warn: no summary.json found for ${case_name}" >&2
    return
  fi

  local achieved_rps
  local p99_ms
  local errors
  achieved_rps=$(awk -F': ' '/\"achieved_rps\"/ {gsub(/,/,"",$2); print $2; exit}' "$summary_path")
  p99_ms=$(awk -F': ' '/\"p99_ms\"/ {gsub(/,/,"",$2); print $2; exit}' "$summary_path")
  errors=$(awk -F': ' '/\"errors\"/ {gsub(/,/,"",$2); print $2; exit}' "$summary_path")

  echo "${case_name},${proxy},${result_path},${achieved_rps},${p99_ms},${errors}" >> "$index"
}

main() {
  if [ "$DRY_RUN" = "1" ] || [ "$DRY_RUN" = "true" ]; then
    echo "========================================"
    echo "  DRY-RUN MODE ENABLED"
    echo "  Setup validation only, no benchmarks"
    echo "========================================"
    echo ""
  fi

  generate_pvs

  # Clean previous results for this proxy
  if [ -d "${RESULTS_DIR}/${PROXY}" ]; then
    echo "Cleaning previous results for ${PROXY}..."
    rm -rf "${RESULTS_DIR}/${PROXY}"
  fi

  mkdir -p "${RESULTS_DIR}/${PROXY}"
  local index
  index=$(index_file)
  echo "case,proxy,result_path,achieved_rps,p99_ms,errors" > "$index"

  for case_name in $CASE; do
    run_case "$case_name" "$PROXY"
    if [ "$DRY_RUN" != "1" ] && [ "$DRY_RUN" != "true" ]; then
      append_index "$index" "$case_name" "$PROXY"
    fi
  done

  if [ "$DRY_RUN" = "1" ] || [ "$DRY_RUN" = "true" ]; then
    echo ""
    echo "========================================"
    echo "  DRY-RUN COMPLETE"
    echo "  All cases validated successfully"
    echo "========================================"
  else
    echo "index written to $index"
  fi
}

main "$@"

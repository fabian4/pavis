#!/usr/bin/env bash
set -euo pipefail

# Benchmark runner: sequentially executes case scripts and writes an index.
# Assumptions:
# - docker and docker compose are available.
# - wrk is installed on the host (for throughput/concurrency/churn tests).
# - bench-loadgen is built (for latency tests) - NO wrk2 required.
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

# Export LOADGEN_BIN path for latency test cases
export LOADGEN_BIN="${ROOT_DIR}/target/release/bench-loadgen"

# PVS config management for pavis
PVS_CONFIG="${ROOT_DIR}/bench/config/pavis.pvs"
YAML_CONFIG="${ROOT_DIR}/bench/config/pavis.yaml"
PVS_GENERATED=false

ensure_loadgen() {
  # Build bench-loadgen if not present (for latency tests)
  if [ ! -x "$LOADGEN_BIN" ]; then
    echo "Building bench-loadgen for latency tests..."
    cargo build -p pavis-benchkit --bin bench-loadgen --release --quiet
  fi
}

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

main() {
  if [ "$DRY_RUN" = "1" ] || [ "$DRY_RUN" = "true" ]; then
    echo "========================================"
    echo "  DRY-RUN MODE ENABLED"
    echo "  Setup validation only, no benchmarks"
    echo "========================================"
    echo ""
  fi

  # Auto-detect CPU pinning availability
  detect_cpu_pinning

  # Ensure bench-loadgen is built (for latency tests)
  ensure_loadgen

  generate_pvs

  # Clean previous results for this proxy
  if [ -d "${RESULTS_DIR}/${PROXY}" ]; then
    echo "Cleaning previous results for ${PROXY}..."
    rm -rf "${RESULTS_DIR}/${PROXY}"
  fi

  mkdir -p "${RESULTS_DIR}/${PROXY}"

  for case_name in $CASE; do
    run_case "$case_name" "$PROXY"
  done

  if [ "$DRY_RUN" = "1" ] || [ "$DRY_RUN" = "true" ]; then
    echo ""
    echo "========================================"
    echo "  DRY-RUN COMPLETE"
    echo "  All cases validated successfully"
    echo "========================================"
  else
    echo ""
    echo "All benchmarks completed for ${PROXY}"
    echo "Results written to ${RESULTS_DIR}/${PROXY}"
    echo ""
    echo "Run 'bash bench/summarize.sh' to generate summary CSV"
  fi
}

main "$@"

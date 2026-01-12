#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"

run_benchmark() {
  load_persisted_env
  : "${BENCH_CASES:?}"
  : "${BENCH_CASES_DIR:?}"
  : "${BENCH_OUTPUT_DIR:?}"
  : "${BENCH_PROXY:?}"
  local open_loop_cases="${BENCH_OPEN_LOOP_CASES:-latency_short_1x latency_extended_1x reload_short_1x}"

  if [[ -d "${BENCH_OUTPUT_DIR}/${BENCH_PROXY}" ]]; then
    log_info "Removing previous results for ${BENCH_PROXY}"
    rm -rf "${BENCH_OUTPUT_DIR:?}/${BENCH_PROXY}"
  fi
  ensure_dir "${BENCH_OUTPUT_DIR}/${BENCH_PROXY}"

  for case_name in $BENCH_CASES; do
    run_case "$case_name" "$open_loop_cases"
  done

  if [[ "$BENCH_DRY_RUN" == "1" || "$BENCH_DRY_RUN" == "true" ]]; then
    log_info "Dry-run mode enabled; skipping summary aggregation"
    return
  fi
}

run_case() {
  local case_name="$1"
  local open_loop_cases="$2"
  local script_path="${BENCH_CASES_DIR}/${case_name}.sh"
  if [[ ! -x "$script_path" ]]; then
    exit_with_error "Case script missing: $script_path"
  fi

  local run_count_override=""
  if [[ -n "${BENCH_OPEN_LOOP_ITERATIONS:-}" && " $open_loop_cases " == *" $case_name "* ]]; then
    run_count_override="$BENCH_OPEN_LOOP_ITERATIONS"
  fi

  # Ensure BENCH_LOADGEN_BIN is available
  if [[ -z "${BENCH_LOADGEN_BIN:-}" ]]; then
      log_warn "BENCH_LOADGEN_BIN not set in environment, attempting fallback"
      BENCH_LOADGEN_BIN="${BENCH_ROOT}/target/release/bench-loadgen"
  fi

  if [[ "$BENCH_DRY_RUN" == "1" || "$BENCH_DRY_RUN" == "true" ]]; then
    log_info "[DRY-RUN] Skipping case ${case_name} (proxy=${BENCH_PROXY})"
    return
  fi

  echo "::group::${case_name} Case 🚀"
  (
    export PROXY="$BENCH_PROXY"
    export DRY_RUN="$BENCH_DRY_RUN"
    export BENCH_VERBOSE="$BENCH_VERBOSE"
    export RUN_COUNT_OVERRIDE="$run_count_override"
    export LOADGEN_BIN="$BENCH_LOADGEN_BIN"
    "$script_path"
  )
  echo "::endgroup::"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  run_benchmark "$@"
fi

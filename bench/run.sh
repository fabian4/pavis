#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export BENCH_ROOT="$ROOT_DIR"
export BENCH_SCRIPTS_DIR="${BENCH_ROOT}/bench/scripts"
export BENCH_STATE_DIR="${BENCH_ROOT}/bench/.bench_state"
mkdir -p "$BENCH_STATE_DIR"

# Early detection of --background flag from command line arguments
for arg in "$@"; do
  if [[ "$arg" == "--background" ]]; then
    export BENCH_BACKGROUND=1
    break
  fi
done

# Handle background mode - restart script with nohup if requested
if [[ "${BENCH_BACKGROUND:-0}" == "1" && "${BENCH_BACKGROUND_ACTIVE:-0}" != "1" ]]; then
  # Source utils for helper functions
  source "$BENCH_SCRIPTS_DIR/utils.sh"

  # Ensure output directory exists
  output_dir="${BENCH_OUTPUT_DIR:-${BENCH_ROOT}/bench/output}"
  mkdir -p "$output_dir"

  # Generate timestamped log file
  log_file="${output_dir}/$(generate_log_filename)"

  # Re-invoke this script with nohup, marking as background-active
  export BENCH_BACKGROUND_ACTIVE=1

  # Preserve all original arguments
  nohup "$0" "$@" > "$log_file" 2>&1 </dev/null &
  bg_pid=$!

  # Disown the process to detach from shell
  disown

  # Print info to user and exit foreground process
  print_background_info "$log_file" "$bg_pid"
  exit 0
fi

# shellcheck source=bench/scripts/utils.sh
source "$BENCH_SCRIPTS_DIR/utils.sh"
# shellcheck source=bench/scripts/requirements.sh
source "$BENCH_SCRIPTS_DIR/requirements.sh"
# shellcheck source=bench/scripts/validate.sh
source "$BENCH_SCRIPTS_DIR/validate.sh"
# shellcheck source=bench/scripts/setup.sh
source "$BENCH_SCRIPTS_DIR/setup.sh"
# shellcheck source=bench/scripts/benchmark.sh
source "$BENCH_SCRIPTS_DIR/benchmark.sh"
# shellcheck source=bench/scripts/cleanup.sh
source "$BENCH_SCRIPTS_DIR/cleanup.sh"
# shellcheck source=bench/scripts/pretty.sh
source "$BENCH_SCRIPTS_DIR/pretty.sh"

cleanup_once=false
cleanup_trap() {
  if [[ "$cleanup_once" == true ]]; then
    return
  fi
  cleanup_once=true
  cleanup_environment || true
}
trap cleanup_trap EXIT

main() {
  check_requirements
  validate_inputs "$@"
  bench_print_benchmark_header "$BENCH_PROXY" "$BENCH_CASES"
  setup_environment
  run_benchmark

  if [[ "$BENCH_DRY_RUN" == "1" || "$BENCH_DRY_RUN" == "true" ]]; then
    log_info "Dry-run completed; report generation skipped"
    return
  fi
}

main "$@"

cleanup_trap
trap - EXIT

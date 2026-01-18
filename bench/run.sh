#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export BENCH_ROOT="$ROOT_DIR"
export BENCH_SCRIPTS_DIR="${BENCH_ROOT}/bench/scripts"
export BENCH_STATE_DIR="${BENCH_ROOT}/bench/.bench_state"
mkdir -p "$BENCH_STATE_DIR"

# Early detection of --background flag and mode from command line arguments
mode_arg=""
for arg in "$@"; do
  if [[ "$arg" == "--background" ]]; then
    export BENCH_BACKGROUND=1
    break
  fi
done
for ((i=1; i<=$#; i++)); do
  if [[ "${!i}" == "--mode" ]]; then
    next_index=$((i+1))
    mode_arg="${!next_index:-}"
  fi
done

# Handle background mode - restart script with nohup if requested
if [[ "${BENCH_BACKGROUND:-0}" == "1" && "${BENCH_BACKGROUND_ACTIVE:-0}" != "1" ]]; then
  # Source utils for helper functions
  source "$BENCH_SCRIPTS_DIR/utils.sh"

  # Ensure output directory exists
  mode_default="${BENCH_MODE:-${MODE:-${mode_arg:-standalone}}}"
  output_dir="${BENCH_OUTPUT_DIR:-${BENCH_ROOT}/bench/output/${mode_default}}"
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

  # Generate run-level context.env for observability and artifact validation
  local mode="${BENCH_MODE:-standalone}"
  local run_output_dir="${BENCH_OUTPUT_DIR}/${mode}"
  if [[ -d "$run_output_dir" ]]; then
    log_info "Cleaning previous output: $run_output_dir"
    rm -rf "$run_output_dir"
  fi
  mkdir -p "$run_output_dir"
  if ! bash "${BENCH_SCRIPTS_DIR}/gen_context_env.sh" "$run_output_dir/context.env"; then
    log_error "Failed to generate context.env"
    exit 1
  fi
  log_info "Generated run-level context.env in $run_output_dir"

  run_benchmark

  if [[ "$BENCH_DRY_RUN" == "1" || "$BENCH_DRY_RUN" == "true" ]]; then
    log_info "Dry-run completed; report generation skipped"
    return
  fi

  if [[ "${BENCH_MODE:-standalone}" == "standalone" ]]; then
    bash "${BENCH_SCRIPTS_DIR}/summarize.sh"
    case "${BENCH_PROFILE:-}" in
      workstation)
        bash "${BENCH_SCRIPTS_DIR}/report_standalone_workstation.sh"
        ;;
      github)
        bash "${BENCH_SCRIPTS_DIR}/report_standalone_github.sh"
        ;;
      *)
        log_warn "Skipping report generation for unknown BENCH_PROFILE=${BENCH_PROFILE:-}"
        ;;
    esac
  fi
}

main "$@"

cleanup_trap
trap - EXIT

#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"

default_cases="throughput_short_1x latency_short_1x latency_extended_1x concurrency_short_1x churn_short_1x reload_short_1x"

validate_inputs() {
  local args=("$@")
  load_persisted_env

  local proxy="${BENCH_PROXY:-${PROXY:-pavis}}"
  local cases="${BENCH_CASES:-${CASE:-$default_cases}}"
  local dry_run="${BENCH_DRY_RUN:-${DRY_RUN:-0}}"
  local verbose="${BENCH_VERBOSE:-0}"
  local runs="${BENCH_BENCHMARK_RUNS:-${BENCHMARK_RUNS:-1}}"
  local open_loop_iterations="${BENCH_OPEN_LOOP_ITERATIONS:-}"
  local output_dir="${BENCH_OUTPUT_DIR:-${BENCH_ROOT}/bench/output}"
  local summary_path="${BENCH_SUMMARY_CSV:-$output_dir/summary.csv}"
  local report_path="${BENCH_REPORT_MD:-$output_dir/report.md}"
  local input_file=""
  local loadgen_warn="${LOADGEN_WARN:-0}"

  local i=0
  while [[ $i -lt ${#args[@]} ]]; do
    case "${args[$i]}" in
      --proxy)
        proxy="${args[$((i+1))]}"
        i=$((i+2))
        ;;
      --cases)
        cases="${args[$((i+1))]}"
        i=$((i+2))
        ;;
      --dry-run)
        dry_run=1
        i=$((i+1))
        ;;
      --verbose)
        verbose=1
        i=$((i+1))
        ;;
      --output)
        output_dir="${args[$((i+1))]}"
        i=$((i+2))
        ;;
      --summary)
        summary_path="${args[$((i+1))]}"
        i=$((i+2))
        ;;
      --report)
        report_path="${args[$((i+1))]}"
        i=$((i+2))
        ;;
      --input)
        input_file="${args[$((i+1))]}"
        i=$((i+2))
        ;;
      --runs)
        runs="${args[$((i+1))]}"
        i=$((i+2))
        ;;
      --open-loop-iterations)
        open_loop_iterations="${args[$((i+1))]}"
        i=$((i+2))
        ;;
      --loadgen-warn)
        loadgen_warn=1
        i=$((i+1))
        ;;
      --help)
        cat <<'USAGE'
Usage: bench/run.sh [options]
  --proxy <name>
  --cases "case1 case2"
  --dry-run
  --verbose
  --output <dir>
  --summary <path>
  --report <path>
  --input <summary.csv>
  --runs <N>
  --open-loop-iterations <N>
  --loadgen-warn
USAGE
        return 0
        ;;
      *)
        log_warn "Ignoring unknown option: ${args[$i]}"
        i=$((i+1))
        ;;
    esac
  done

  ensure_dir "$output_dir"

  if [[ -z "$cases" ]]; then
    exit_with_error "No benchmark cases specified"
  fi

  local cases_dir="${BENCH_ROOT}/bench/cases"
  for case_name in $cases; do
    local script_path="$cases_dir/${case_name}.sh"
    if [[ ! -x "$script_path" ]]; then
      exit_with_error "Missing case script: $script_path"
    fi
  done

  if [[ -n "$input_file" && ! -f "$input_file" ]]; then
    exit_with_error "Input file not found: $input_file"
  fi

  if [[ "$proxy" == "pavis" ]]; then
    local pavis_yaml="${BENCH_ROOT}/bench/config/pavis.yaml"
    if [[ ! -f "$pavis_yaml" ]]; then
      exit_with_error "Pavis config not found: $pavis_yaml"
    fi
  fi

  export BENCH_PROXY="$proxy"
  export BENCH_CASES="$cases"
  export BENCH_DRY_RUN="$dry_run"
  export BENCH_VERBOSE="$verbose"
  export BENCH_BENCHMARK_RUNS="$runs"
  export BENCH_OPEN_LOOP_ITERATIONS="${open_loop_iterations:-}"
  export BENCH_OUTPUT_DIR="$output_dir"
  export BENCH_SUMMARY_CSV="$summary_path"
  export BENCH_REPORT_MD="$report_path"
  export BENCH_CASES_DIR="$cases_dir"
  export BENCH_SCRIPTS_DIR="$SCRIPT_DIR"
  export BENCH_LOADGEN_BIN="${BENCH_ROOT}/target/release/bench-loadgen"
  if [[ -f "${BENCH_LOADGEN_BIN}.exe" ]]; then
    export BENCH_LOADGEN_BIN="${BENCH_LOADGEN_BIN}.exe"
  fi
  export BENCH_PVS_CONFIG="${BENCH_ROOT}/bench/config/pavis.pvs"
  export BENCH_DOCKER_COMPOSE="${BENCH_ROOT}/bench/docker-compose.yaml"
  export LOADGEN_WARN="$loadgen_warn"

  log_info "Proxy: $BENCH_PROXY"
  log_info "Cases: $BENCH_CASES"
  log_info "Output directory: $BENCH_OUTPUT_DIR"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  if [[ -z "${BENCH_ROOT:-}" ]]; then
    exit_with_error "BENCH_ROOT must be set before running validation"
  fi
  validate_inputs "$@"
fi

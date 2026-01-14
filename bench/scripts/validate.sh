#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"

default_cases="throughput_short_1x latency_short_1x latency_extended_1x concurrency_short_1x churn_short_1x"
validate_inputs() {
  local args=("$@")
  load_persisted_env

  local proxy="${BENCH_PROXY:-${PROXY:-pavis}}"
  local cases="${BENCH_CASES:-${CASE:-$default_cases}}"
  local dry_run="${BENCH_DRY_RUN:-${DRY_RUN:-0}}"
  local verbose="${BENCH_VERBOSE:-0}"
  local runs="${BENCH_BENCHMARK_RUNS:-${BENCHMARK_RUNS:-1}}"
  local open_loop_iterations="${BENCH_OPEN_LOOP_ITERATIONS:-}"
  local profile="${BENCH_PROFILE:-}"
  local mode="${BENCH_MODE:-${MODE:-}}"
  local payload_size="${BENCH_PAYLOAD_SIZE:-64B}"
  local bench_tls="${BENCH_TLS:-false}"
  local bench_metrics="${BENCH_METRICS:-false}"
  local output_dir="${BENCH_OUTPUT_DIR:-${BENCH_ROOT}/bench/output}"
  local summary_path="${BENCH_SUMMARY_CSV:-$output_dir/summary.csv}"
  local report_path="${BENCH_REPORT_MD:-$output_dir/report.md}"
  local input_file=""
  local loadgen_warn="${LOADGEN_WARN:-0}"
  local background="${BENCH_BACKGROUND:-0}"

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
      --profile)
        profile="${args[$((i+1))]}"
        i=$((i+2))
        ;;
      --mode)
        mode="${args[$((i+1))]}"
        i=$((i+2))
        ;;
      --payload-size)
        payload_size="${args[$((i+1))]}"
        i=$((i+2))
        ;;
      --tls)
        bench_tls="${args[$((i+1))]}"
        i=$((i+2))
        ;;
      --metrics)
        bench_metrics="${args[$((i+1))]}"
        i=$((i+2))
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
      --background)
        background=1
        i=$((i+1))
        ;;
      --help)
        cat <<'USAGE'
Usage: bench/run.sh [options]
  --proxy <name>
  --cases "case1 case2"
  --dry-run
  --profile <github|workstation>
  --mode <standalone|system>
  --payload-size <size>
  --tls <true|false>
  --metrics <true|false>
  --verbose
  --output <dir>
  --summary <path>
  --report <path>
  --input <summary.csv>
  --runs <N>
  --open-loop-iterations <N>
  --loadgen-warn
  --background              Run in background mode with persistent logging
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

  if [[ -z "$profile" ]]; then
    case "${IS_CI:-}" in
      1|true|TRUE|yes|YES)
        profile="github"
        ;;
      *)
        profile="workstation"
        ;;
    esac
  fi

  if [[ "$profile" == "ci" ]]; then
    log_warn "BENCH_PROFILE=ci is deprecated; use BENCH_PROFILE=github"
    profile="github"
  fi

  if [[ "$profile" != "github" && "$profile" != "workstation" ]]; then
    exit_with_error "Invalid BENCH_PROFILE: $profile (expected github or workstation)"
  fi

  if [[ -z "$mode" ]]; then
    log_info "MODE not set, will run both standalone and system modes"
    mode="both"
  fi

  if [[ "$mode" != "standalone" && "$mode" != "system" && "$mode" != "both" ]]; then
    exit_with_error "Invalid MODE: $mode (expected standalone, system, or unset for both)"
  fi

  # System mode constraints
  if [[ "$mode" == "system" || "$mode" == "both" ]]; then
    if [[ "$profile" == "github" ]]; then
      exit_with_error "MODE=system requires BENCH_PROFILE=workstation (not supported in GitHub CI)"
    fi
  fi

  # If running both modes, validate both case directories exist
  if [[ "$mode" == "both" ]]; then
    local standalone_cases_dir="${BENCH_ROOT}/bench/cases/standalone"
    local system_cases_dir="${BENCH_ROOT}/bench/cases/system"

    if [[ ! -d "$standalone_cases_dir" ]]; then
      exit_with_error "Standalone cases directory not found: $standalone_cases_dir"
    fi

    if [[ ! -d "$system_cases_dir" ]]; then
      exit_with_error "System cases directory not found: $system_cases_dir"
    fi

    # For both mode, we'll use standalone cases dir as default
    # System mode will override this in its execution
    cases_dir="$standalone_cases_dir"
  elif [[ "$mode" == "system" ]]; then
    cases_dir="${BENCH_ROOT}/bench/cases/system"
  else
    cases_dir="${BENCH_ROOT}/bench/cases/standalone"
  fi

  for case_name in $cases; do
    local script_path="$cases_dir/${case_name}.sh"
    if [[ ! -x "$script_path" ]]; then
      exit_with_error "Missing case script: $script_path"
    fi
  done

  if [[ "$profile" == "github" && "$proxy" != "pavis" ]]; then
    exit_with_error "BENCH_PROFILE=github only supports PROXY=pavis"
  fi

  if [[ "$profile" == "github" ]]; then
    case "$bench_tls" in
      1|true|TRUE|True|yes|YES|Yes|y|Y)
        exit_with_error "BENCH_TLS is not permitted under BENCH_PROFILE=github"
        ;;
    esac
    case "$bench_metrics" in
      1|true|TRUE|True|yes|YES|Yes|y|Y)
        exit_with_error "BENCH_METRICS is not permitted under BENCH_PROFILE=github"
        ;;
    esac
    if [[ "$payload_size" != "64B" ]]; then
      exit_with_error "BENCH_PAYLOAD_SIZE must remain 64B under BENCH_PROFILE=github"
    fi
    local filtered_cases=""
    for case_name in $cases; do
      if [[ "$case_name" == "latency_extended_1x" ]]; then
        log_warn "Skipping latency_extended_1x for BENCH_PROFILE=github"
        continue
      fi
      filtered_cases+="${case_name} "
    done
    cases="${filtered_cases% }"
    if [[ -z "$cases" ]]; then
      exit_with_error "No cases remain after BENCH_PROFILE=github gating"
    fi
  fi

  if [[ "$profile" == "workstation" && "$cases" == "$default_cases" ]]; then
    log_info "Workstation profile runs payload matrix for throughput/latency cases"
  fi

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
  export BENCH_PROFILE="$profile"
  export BENCH_MODE="$mode"
  export BENCH_PAYLOAD_SIZE="$payload_size"
  export BENCH_TLS="$bench_tls"
  export BENCH_METRICS="$bench_metrics"
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
  export BENCH_BACKGROUND="$background"

  log_info "Proxy: $BENCH_PROXY"
  log_info "Cases: $BENCH_CASES"
  log_info "Profile: $BENCH_PROFILE"
  log_info "Mode: $BENCH_MODE"
  log_info "Payload size: $BENCH_PAYLOAD_SIZE"
  log_info "TLS: $BENCH_TLS"
  log_info "Metrics: $BENCH_METRICS"
  log_info "Output directory: $BENCH_OUTPUT_DIR"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  if [[ -z "${BENCH_ROOT:-}" ]]; then
    exit_with_error "BENCH_ROOT must be set before running validation"
  fi
  validate_inputs "$@"
fi

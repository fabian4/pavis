#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"
# shellcheck source=scripts/lib/contract.sh
source "$SCRIPT_DIR/../../scripts/lib/contract.sh"

run_benchmark() {
  load_persisted_env
  : "${BENCH_CASES:?}"
  : "${BENCH_CASES_DIR:?}"
  : "${BENCH_OUTPUT_DIR:?}"
  : "${BENCH_PROXY:?}"
  local open_loop_cases="${BENCH_OPEN_LOOP_CASES:-latency_short_1x latency_extended_1x}"
  local failed_cases=()

  # System mode uses different output directory structure
  local mode="${BENCH_MODE:-standalone}"
  local output_subdir="${BENCH_OUTPUT_DIR}/${mode}/${BENCH_PROXY}"

  if [[ -d "$output_subdir" ]]; then
    log_warn "Removing previous results for ${BENCH_PROXY} (${mode} mode)"
    rm -rf "${output_subdir}"
  fi
  ensure_dir "$output_subdir"
  if ! bash "${BENCH_SCRIPTS_DIR}/gen_context_env.sh" "$output_subdir/context.env"; then
    exit_with_error "Failed to generate context.env for ${BENCH_PROXY}"
  fi

  for case_name in $BENCH_CASES; do
    if ! run_case "$case_name" "$open_loop_cases"; then
      failed_cases+=("$case_name")
    fi
  done

  if [[ ${#failed_cases[@]} -gt 0 ]]; then
    log_error "The following cases failed validation: ${failed_cases[*]}"
    return 1
  fi

  if [[ "$BENCH_DRY_RUN" == "1" || "$BENCH_DRY_RUN" == "true" ]]; then
    log_info "Dry-run mode enabled; skipping summary aggregation"
    return
  fi
}

format_mem_mib() {
  local kib="$1"
  if [[ -z "$kib" ]]; then
    echo "unknown"
    return
  fi
  awk -v v="$kib" 'BEGIN {printf "%.0fMiB", v/1024}'
}

count_cpuset_string() {
  local cpuset="$1"
  if [[ -z "$cpuset" ]]; then
    echo "0"
    return
  fi
  awk -v s="$cpuset" 'BEGIN {
    n=split(s, parts, ",");
    total=0;
    for (i=1; i<=n; i++) {
      p=parts[i];
      if (p ~ /^[0-9]+-[0-9]+$/) {
        split(p, r, "-");
        a=r[1]+0; b=r[2]+0;
        if (a<=b) total+=b-a+1; else total+=a-b+1;
      } else if (p ~ /^[0-9]+$/) {
        total+=1;
      }
    }
    print total;
  }'
}

payload_size_to_bytes() {
  local value="$1"
  local upper
  upper=$(printf '%s' "$value" | tr '[:lower:]' '[:upper:]')
  if [[ "$upper" =~ ^([0-9]+)KIB$ ]]; then
    echo "$((10#${BASH_REMATCH[1]} * 1024))"
  elif [[ "$upper" =~ ^([0-9]+)KB$ ]]; then
    echo "$((10#${BASH_REMATCH[1]} * 1000))"
  elif [[ "$upper" =~ ^([0-9]+)MIB$ ]]; then
    echo "$((10#${BASH_REMATCH[1]} * 1024 * 1024))"
  elif [[ "$upper" =~ ^([0-9]+)MB$ ]]; then
    echo "$((10#${BASH_REMATCH[1]} * 1000 * 1000))"
  elif [[ "$upper" =~ ^([0-9]+)B$ ]]; then
    echo "$((10#${BASH_REMATCH[1]}))"
  else
    echo ""
  fi
}

log_case_environment() {
  local case_name="$1"
  local mem_kib=""
  local cpuset_effective="unknown"
  local effective_cores="0"
  local docker_cpu_limit="${CPU_LIMIT:-}"
  local docker_mem_limit="${MEMORY_LIMIT:-}"
  local proxy_pin="${PROXY_CPUSET:-none}"
  local backend_pin="${BACKEND_CPUSET:-none}"
  local loadgen_pin="${BENCH_LOADGEN_CPUSET:-none}"
  if [[ -r /proc/meminfo ]]; then
    mem_kib=$(awk '/MemTotal:/ {print $2; exit}' /proc/meminfo)
  fi
  local mem_total
  mem_total=$(format_mem_mib "$mem_kib")
  if [[ -r /sys/fs/cgroup/cpuset.cpus.effective ]]; then
    cpuset_effective=$(cat /sys/fs/cgroup/cpuset.cpus.effective 2>/dev/null || echo "unknown")
  elif [[ -r /sys/fs/cgroup/cpuset/cpuset.cpus.effective ]]; then
    cpuset_effective=$(cat /sys/fs/cgroup/cpuset/cpuset.cpus.effective 2>/dev/null || echo "unknown")
  fi
  if [[ "$cpuset_effective" != "unknown" ]]; then
    effective_cores=$(count_cpuset_string "$cpuset_effective")
  fi
  if [[ "$proxy_pin" != "none" ]]; then
    proxy_pin="{${proxy_pin//,/\,}}"
  fi

  export BENCH_HOST_CORES="${effective_cores}"
  export BENCH_HOST_CPUSET_EFFECTIVE="${cpuset_effective}"
  export BENCH_HOST_MEM_TOTAL="${mem_total}"
  export BENCH_PROXY_CPU_LIMIT="${docker_cpu_limit:-unset}"
  export BENCH_PROXY_MEM_LIMIT="${docker_mem_limit:-unset}"

  log_info "Case ${case_name} on ${BENCH_PROXY}"
  log_info "Host capacity | cores=${effective_cores} (cpuset=${cpuset_effective:-unknown}), mem=${mem_total}"
  log_info "CPU pinning   | backend=${backend_pin}, proxy=${proxy_pin}, loadgen=${loadgen_pin}"
  log_info "Limits        | proxy_cpu=${docker_cpu_limit:-unset}, proxy_mem=${docker_mem_limit:-unset}"
  log_info "Environment   | compose=${BENCH_DOCKER_COMPOSE:-unset}"
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

  local payload_set=("$BENCH_PAYLOAD_SIZE")
  if [[ "${BENCH_PROFILE:-}" == "workstation" ]]; then
    if [[ "$case_name" == "throughput_short_1x" || "$case_name" == "latency_short_1x" || "$case_name" == "latency_extended_1x" ]]; then
      payload_set=("64B" "4KiB")
    fi
  fi

  for payload_size in "${payload_set[@]}"; do
    local payload_bytes
    payload_bytes=$(payload_size_to_bytes "$payload_size")
    if [[ -z "$payload_bytes" ]]; then
      exit_with_error "Invalid BENCH_PAYLOAD_SIZE: $payload_size"
    fi

    local payload_suffix
    payload_suffix=$(printf '%s' "$payload_size" | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9' '_')
    export BENCH_CASE_SUFFIX="payload_${payload_suffix}"
    persist_env_var "BENCH_CASE_SUFFIX" "$BENCH_CASE_SUFFIX"
    export BENCH_PAYLOAD_SIZE="$payload_size"
    export BENCH_PAYLOAD_BYTES="$payload_bytes"
    persist_env_var "BENCH_PAYLOAD_SIZE" "$BENCH_PAYLOAD_SIZE"
    persist_env_var "BENCH_PAYLOAD_BYTES" "$BENCH_PAYLOAD_BYTES"

    log_case_environment "$case_name"

    if [[ "$BENCH_DRY_RUN" == "1" || "$BENCH_DRY_RUN" == "true" ]]; then
      log_info "[DRY-RUN] Skipping case ${case_name} (proxy=${BENCH_PROXY})"
      continue
    fi

    echo "::group::${case_name} Case 🚀"
    local case_status=0
    set +e
    (
      export PROXY="$BENCH_PROXY"
      export DRY_RUN="$BENCH_DRY_RUN"
      export BENCH_VERBOSE="$BENCH_VERBOSE"
      export RUN_COUNT_OVERRIDE="$run_count_override"
      export LOADGEN_BIN="$BENCH_LOADGEN_BIN"
      export BENCH_PROFILE="$BENCH_PROFILE"
      export BENCH_MODE="$BENCH_MODE"
      export BENCH_PAYLOAD_SIZE="$BENCH_PAYLOAD_SIZE"
      export BENCH_PAYLOAD_BYTES="$BENCH_PAYLOAD_BYTES"
      export BENCH_TLS="$BENCH_TLS"
      export BENCH_METRICS="$BENCH_METRICS"
      export BACKEND_CPUSET="${BACKEND_CPUSET:-}"
      export PROXY_CPUSET="${PROXY_CPUSET:-}"
      export BENCH_LOADGEN_CPUSET="${BENCH_LOADGEN_CPUSET:-}"
      "$script_path"
    )
    case_status=$?
    set -e
    echo "::endgroup::"

    local mode="${BENCH_MODE:-standalone}"
    local case_dir_base="${BENCH_OUTPUT_DIR}/${mode}/${BENCH_PROXY}/${case_name}"
    local case_dir="${case_dir_base}${BENCH_CASE_SUFFIX:+__${BENCH_CASE_SUFFIX}}"
    if [[ ! -d "$case_dir" && -d "$case_dir_base" ]]; then
      case_dir="$case_dir_base"
    fi

    if [[ $case_status -ne 0 ]]; then
      log_error "Case ${case_name} failed with exit code ${case_status}"
      if [[ -d "$case_dir" ]]; then
        touch "$case_dir/.validation_failed"
      else
        log_warn "Case directory missing; cannot mark validation failure: $case_dir"
      fi
      return 1
    fi

    if [[ "$BENCH_DRY_RUN" != "1" && "$BENCH_DRY_RUN" != "true" ]]; then
      if [[ ! -d "$case_dir" ]]; then
        log_warn "Case output missing; skipping validation for $case_name"
      elif [[ -f "$case_dir/.skipped" ]]; then
        log_warn "Case marked skipped; skipping validation for $case_name"
      elif ! validate_benchmark_artifacts "$case_name" "$case_dir"; then
        log_error "Artifact validation failed for $case_name"
        touch "$case_dir/.validation_failed"
        return 1
      else
        log_debug "Artifact validation passed for $case_name"
      fi
    fi
  done
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  run_benchmark "$@"
fi

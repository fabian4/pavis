#!/usr/bin/env bash
set -euo pipefail

# Generate shell-sourceable context.env for benchmark runs
# Usage: ./gen_context_env.sh <output_file>
# Output format: KEY=value (safely quoted with printf '%s=%q\n')

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/log.sh
source "$SCRIPT_DIR/../../scripts/lib/log.sh"
# shellcheck source=scripts/lib/time.sh
source "$SCRIPT_DIR/../../scripts/lib/time.sh"

main() {
  local output_file="${1:-}"

  if [[ -z "$output_file" ]]; then
    log_error "Usage: $0 <output_file>"
    exit 1
  fi

  local bench_root
  bench_root="${BENCH_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

  mkdir -p "$(dirname "$output_file")"

  local run_timestamp
  run_timestamp="$(timestamp_iso8601)"
  local git_sha
  git_sha="$(git -C "$bench_root" rev-parse HEAD 2>/dev/null || echo "unknown")"
  local run_tag
  run_tag="$(git -C "$bench_root" rev-parse --short HEAD 2>/dev/null || echo "unknown")"

  local kernel
  kernel="$(uname -r)"
  local cpu_model
  cpu_model="$(awk -F: '/model name/ {print $2; exit}' /proc/cpuinfo 2>/dev/null | sed 's/^ *//' || echo "unknown")"
  local cpu_governor="unknown"
  if [[ -r /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor ]]; then
    cpu_governor="$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)"
  fi

  local bench_host_cores
  bench_host_cores="$(nproc 2>/dev/null || echo "unknown")"
  local bench_host_cpuset_effective
  bench_host_cpuset_effective="$(awk '/Cpus_allowed_list:/ {print $2}' /proc/self/status 2>/dev/null || echo "unknown")"
  local bench_host_mem_total
  bench_host_mem_total="$(awk '/MemTotal/ {print $2}' /proc/meminfo 2>/dev/null || echo "unknown")"

  local bench_mode="${BENCH_MODE:-standalone}"
  local bench_profile="${BENCH_PROFILE:-}"
  local bench_proxy="${BENCH_PROXY:-pavis}"
  local bench_payload_size="${BENCH_PAYLOAD_SIZE:-}"
  local bench_tls="${BENCH_TLS:-}"
  local bench_metrics="${BENCH_METRICS:-}"
  local bench_docker_compose="${BENCH_DOCKER_COMPOSE:-}"

  local bench_dry_run="${BENCH_DRY_RUN:-0}"
  local bench_verbose="${BENCH_VERBOSE:-0}"
  local bench_cases="${BENCH_CASES:-}"
  local bench_output_dir="${BENCH_OUTPUT_DIR:-${bench_root}/bench/output}"
  local bench_scripts_dir="${BENCH_SCRIPTS_DIR:-${bench_root}/bench/scripts}"
  local bench_cases_dir="${BENCH_CASES_DIR:-${bench_root}/bench/cases/${bench_mode}}"
  local bench_loadgen_bin="${BENCH_LOADGEN_BIN:-${bench_root}/target/release/bench-loadgen}"
  local bench_pvs_config="${BENCH_PVS_CONFIG:-${bench_root}/bench/config/pavis.pvs}"

  local backend_cpuset="${BACKEND_CPUSET:-}"
  local proxy_cpuset="${PROXY_CPUSET:-}"
  local bench_loadgen_cpuset="${BENCH_LOADGEN_CPUSET:-}"
  local bench_proxy_cpu_limit="${BENCH_PROXY_CPU_LIMIT:-}"
  local bench_proxy_mem_limit="${BENCH_PROXY_MEM_LIMIT:-}"

  {
    printf '%s=%q\n' "RUN_TIMESTAMP" "$run_timestamp"
    printf '%s=%q\n' "GIT_SHA" "$git_sha"
    printf '%s=%q\n' "RUN_TAG" "$run_tag"

    printf '%s=%q\n' "BENCH_MODE" "$bench_mode"
    printf '%s=%q\n' "BENCH_PROFILE" "$bench_profile"
    printf '%s=%q\n' "BENCH_PROXY" "$bench_proxy"
    printf '%s=%q\n' "BENCH_PAYLOAD_SIZE" "$bench_payload_size"
    printf '%s=%q\n' "BENCH_TLS" "$bench_tls"
    printf '%s=%q\n' "BENCH_METRICS" "$bench_metrics"
    printf '%s=%q\n' "BENCH_DRY_RUN" "$bench_dry_run"
    printf '%s=%q\n' "BENCH_VERBOSE" "$bench_verbose"
    printf '%s=%q\n' "BENCH_CASES" "$bench_cases"

    printf '%s=%q\n' "BENCH_DOCKER_COMPOSE" "$bench_docker_compose"
    printf '%s=%q\n' "BENCH_LOADGEN_BIN" "$bench_loadgen_bin"
    printf '%s=%q\n' "BENCH_PVS_CONFIG" "$bench_pvs_config"

    printf '%s=%q\n' "BACKEND_CPUSET" "$backend_cpuset"
    printf '%s=%q\n' "PROXY_CPUSET" "$proxy_cpuset"
    printf '%s=%q\n' "BENCH_LOADGEN_CPUSET" "$bench_loadgen_cpuset"
    printf '%s=%q\n' "BENCH_PROXY_CPU_LIMIT" "$bench_proxy_cpu_limit"
    printf '%s=%q\n' "BENCH_PROXY_MEM_LIMIT" "$bench_proxy_mem_limit"

    printf '%s=%q\n' "BENCH_HOST_CORES" "$bench_host_cores"
    printf '%s=%q\n' "BENCH_HOST_CPUSET_EFFECTIVE" "$bench_host_cpuset_effective"
    printf '%s=%q\n' "BENCH_HOST_MEM_TOTAL" "$bench_host_mem_total"
    printf '%s=%q\n' "BENCH_HOST_CPU_MODEL" "$cpu_model"
    printf '%s=%q\n' "BENCH_HOST_CPU_GOVERNOR" "$cpu_governor"
    printf '%s=%q\n' "BENCH_HOST_KERNEL" "$kernel"

    printf '%s=%q\n' "BENCH_ROOT" "$bench_root"
    printf '%s=%q\n' "BENCH_SCRIPTS_DIR" "$bench_scripts_dir"
    printf '%s=%q\n' "BENCH_OUTPUT_DIR" "$bench_output_dir"
    printf '%s=%q\n' "BENCH_CASES_DIR" "$bench_cases_dir"
  } > "$output_file"

  log_info "Generated benchmark context: $output_file"
}

main "$@"

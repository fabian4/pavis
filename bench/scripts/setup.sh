#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"
# shellcheck source=bench/scripts/pretty.sh
source "$SCRIPT_DIR/pretty.sh"

setup_environment() {
  load_persisted_env
  : "${BENCH_ROOT:?BENCH_ROOT is required}"
  : "${BENCH_PROXY:?BENCH_PROXY is required}"
  : "${BENCH_LOADGEN_BIN:?BENCH_LOADGEN_BIN is required}"
  : "${BENCH_PVS_CONFIG:?BENCH_PVS_CONFIG is required}"
  : "${BENCH_DOCKER_COMPOSE:?BENCH_DOCKER_COMPOSE is required}"

  bench_print_step "Preparing benchmark environment"

  if [[ ! -x "$BENCH_LOADGEN_BIN" ]]; then
    bench_print_step "Building bench-loadgen binary"
    cargo build -p pavis-benchkit --bin bench-loadgen --release
  fi

  detect_cpu_pinning

  if [[ "$BENCH_PROXY" == "pavis" ]]; then
    ensure_pavis_config
  else
    BENCH_PVS_GENERATED=false
  fi

  export BACKEND_CPUSET
  export PROXY_CPUSET
  export BENCH_PVS_GENERATED

  persist_env_var "BACKEND_CPUSET" "${BACKEND_CPUSET:-}"
  persist_env_var "PROXY_CPUSET" "${PROXY_CPUSET:-}"
  persist_env_var "BENCH_PVS_GENERATED" "${BENCH_PVS_GENERATED:-false}"

  local host_info="${BENCH_OUTPUT_DIR}/.host_info"
  collect_system_info > "$host_info"
  export BENCH_HOST_INFO="$host_info"
  persist_env_var "BENCH_HOST_INFO" "$host_info"
  persist_env_var "BENCH_LOADGEN_BIN" "$BENCH_LOADGEN_BIN"
}

ensure_pavis_config() {
  BENCH_PVS_GENERATED=false
  if [[ ! -f "$BENCH_PVS_CONFIG" ]]; then
    bench_print_step "Generating .pvs config from ${BENCH_ROOT}/bench/config/pavis.yaml"
    local pavctl="${BENCH_ROOT}/target/release/pavctl"
    if [[ ! -x "$pavctl" ]]; then
      bench_print_step "Building pavctl"
      cargo build -p pavctl --release
    fi
    "$pavctl" gen "${BENCH_ROOT}/bench/config/pavis.yaml" "$BENCH_PVS_CONFIG"
    BENCH_PVS_GENERATED=true
  fi
}

detect_cpu_pinning() {
  BACKEND_CPUSET="${BACKEND_CPUSET:-}"
  PROXY_CPUSET="${PROXY_CPUSET:-}"
  local cpuset_file=""
  if [[ -f /sys/fs/cgroup/cpuset/cpuset.cpus ]]; then
    cpuset_file=/sys/fs/cgroup/cpuset/cpuset.cpus
  elif [[ -f /sys/fs/cgroup/cpuset.cpus ]]; then
    cpuset_file=/sys/fs/cgroup/cpuset.cpus
  fi

  if [[ -z "$cpuset_file" ]]; then
    BACKEND_CPUSET="${BACKEND_CPUSET:-0}"
    PROXY_CPUSET="${PROXY_CPUSET:-1-2}"
    return
  fi

  local available
  available=$(cat "$cpuset_file" 2>/dev/null || true)
  if [[ -z "$BACKEND_CPUSET" ]]; then
    if grep -qE '(^|,)0(,|$|-)' <<< "$available"; then
      BACKEND_CPUSET=0
    fi
  fi
  if [[ -z "$PROXY_CPUSET" ]]; then
    if grep -qE '(^|,)1(-2)?(,|$)' <<< "$available"; then
      PROXY_CPUSET=1-2
    fi
  fi
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  setup_environment "$@"
fi

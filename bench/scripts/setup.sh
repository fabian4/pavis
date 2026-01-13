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

  if [[ "${BENCH_PROFILE:-}" == "github" ]]; then
    BACKEND_CPUSET=""
    PROXY_CPUSET=""
    BENCH_LOADGEN_CPUSET=""
  fi

  if [[ "${BENCH_PROFILE:-}" == "workstation" ]]; then
    require_cmd taskset
    if [[ -z "${BACKEND_CPUSET:-}" || -z "${PROXY_CPUSET:-}" || -z "${BENCH_LOADGEN_CPUSET:-}" ]]; then
      exit_with_error "CPU pinning is required for BENCH_PROFILE=workstation (set BACKEND_CPUSET/PROXY_CPUSET/BENCH_LOADGEN_CPUSET)"
    fi
    local backend_cpu_count
    local proxy_cpu_count
    local loadgen_cpu_count
    backend_cpu_count=$(count_cpuset "$BACKEND_CPUSET")
    proxy_cpu_count=$(count_cpuset "$PROXY_CPUSET")
    loadgen_cpu_count=$(count_cpuset "$BENCH_LOADGEN_CPUSET")
    if [[ "$backend_cpu_count" != "1" || "$proxy_cpu_count" != "2" || "$loadgen_cpu_count" != "1" ]]; then
      exit_with_error "Workstation profile requires 4 dedicated cores (1 loadgen, 1 upstream, 2 proxy)"
    fi
    if [[ -z "${CPU_LIMIT:-}" ]]; then
      proxy_cpu_count=$(count_cpuset "$PROXY_CPUSET")
      if [[ -n "$proxy_cpu_count" ]]; then
        export CPU_LIMIT="$proxy_cpu_count"
        persist_env_var "CPU_LIMIT" "$CPU_LIMIT"
      fi
    fi
    if [[ -z "${BACKEND_CPU_LIMIT:-}" ]]; then
      backend_cpu_count=$(count_cpuset "$BACKEND_CPUSET")
      if [[ -n "$backend_cpu_count" ]]; then
        export BACKEND_CPU_LIMIT="$backend_cpu_count"
        persist_env_var "BACKEND_CPU_LIMIT" "$BACKEND_CPU_LIMIT"
      fi
    fi
    if [[ -z "${MEMORY_LIMIT:-}" ]]; then
      export MEMORY_LIMIT="1G"
      persist_env_var "MEMORY_LIMIT" "$MEMORY_LIMIT"
    fi
  fi

  if [[ "$BENCH_PROXY" == "pavis" ]]; then
    ensure_pavis_config
  else
    BENCH_PVS_GENERATED=false
  fi

  export BACKEND_CPUSET
  export PROXY_CPUSET
  export BENCH_LOADGEN_CPUSET
  export BENCH_PVS_GENERATED

  persist_env_var "BACKEND_CPUSET" "${BACKEND_CPUSET:-}"
  persist_env_var "PROXY_CPUSET" "${PROXY_CPUSET:-}"
  persist_env_var "BENCH_LOADGEN_CPUSET" "${BENCH_LOADGEN_CPUSET:-}"
  persist_env_var "BENCH_PVS_GENERATED" "${BENCH_PVS_GENERATED:-false}"

  local host_info="${BENCH_OUTPUT_DIR}/.host_info"
  collect_system_info > "$host_info"
  export BENCH_HOST_INFO="$host_info"
  persist_env_var "BENCH_HOST_INFO" "$host_info"
  persist_env_var "BENCH_LOADGEN_BIN" "$BENCH_LOADGEN_BIN"
}

count_cpuset() {
  local cpuset="$1"
  if [[ -z "$cpuset" ]]; then
    echo ""
    return
  fi
  local count
  count=$(_expand_cpuset "$cpuset" | sort -n | uniq | wc -l | tr -d ' ')
  echo "$count"
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

# Expand cpuset string like "0-1,4,6-7" into lines: 0 1 4 6 7
_expand_cpuset() {
  local s="${1//[[:space:]]/}"
  local part a b
  IFS=',' read -r -a parts <<< "$s"
  for part in "${parts[@]}"; do
    [[ -z "$part" ]] && continue
    if [[ "$part" =~ ^([0-9]+)-([0-9]+)$ ]]; then
      a="${BASH_REMATCH[1]}"; b="${BASH_REMATCH[2]}"
      if (( a <= b )); then
        for ((i=a; i<=b; i++)); do echo "$i"; done
      else
        for ((i=a; i>=b; i--)); do echo "$i"; done
      fi
    elif [[ "$part" =~ ^[0-9]+$ ]]; then
      echo "$part"
    fi
  done
}

detect_cpu_pinning() {
  BACKEND_CPUSET="${BACKEND_CPUSET:-}"
  PROXY_CPUSET="${PROXY_CPUSET:-}"
  BENCH_LOADGEN_CPUSET="${BENCH_LOADGEN_CPUSET:-}"

  # If user explicitly set either, respect it and return
  if [[ -n "$BACKEND_CPUSET" || -n "$PROXY_CPUSET" || -n "$BENCH_LOADGEN_CPUSET" ]]; then
    return
  fi

  local cpuset_file=""
  local cpuset_effective_file=""
  if [[ -f /sys/fs/cgroup/cpuset/cpuset.cpus ]]; then
    cpuset_file=/sys/fs/cgroup/cpuset/cpuset.cpus
  elif [[ -f /sys/fs/cgroup/cpuset.cpus ]]; then
    cpuset_file=/sys/fs/cgroup/cpuset.cpus
  fi
  if [[ -f /sys/fs/cgroup/cpuset.cpus.effective ]]; then
    cpuset_effective_file=/sys/fs/cgroup/cpuset.cpus.effective
  elif [[ -f /sys/fs/cgroup/cpuset/cpuset.cpus.effective ]]; then
    cpuset_effective_file=/sys/fs/cgroup/cpuset/cpuset.cpus.effective
  fi

  # If we cannot detect, default to "workstation dev" assumption
  if [[ -z "$cpuset_file" ]]; then
    BACKEND_CPUSET=0
    PROXY_CPUSET=1-2
    return
  fi

  local available
  available="$(cat "$cpuset_file" 2>/dev/null || true)"
  available="${available//$'\n'/}"
  if [[ -z "$available" && -n "$cpuset_effective_file" ]]; then
    available="$(cat "$cpuset_effective_file" 2>/dev/null || true)"
    available="${available//$'\n'/}"
  fi
  if [[ -z "$available" && -x "$(command -v nproc)" ]]; then
    local cpu_count
    cpu_count=$(nproc 2>/dev/null || echo "")
    if [[ "$cpu_count" =~ ^[0-9]+$ && "$cpu_count" -gt 0 ]]; then
      available="0-$((cpu_count - 1))"
    fi
  fi
  if [[ -z "$available" ]]; then
    # Unknown: safest is to disable pinning rather than guess
    BACKEND_CPUSET=""
    PROXY_CPUSET=""
    return
  fi

  # If effective cpuset is narrower, prefer it for pinning.
  if [[ -n "$cpuset_effective_file" ]]; then
    local effective
    effective="$(cat "$cpuset_effective_file" 2>/dev/null || true)"
    effective="${effective//$'\n'/}"
    if [[ -n "$effective" ]]; then
      available="$effective"
    fi
  fi

  # Build sorted unique cpu list
  mapfile -t cpus < <(_expand_cpuset "$available" | sort -n | uniq)
  local n="${#cpus[@]}"

  if (( n >= 4 )); then
    # Pin: backend to first cpu, proxy to next two, loadgen to fourth.
    BACKEND_CPUSET="${cpus[0]}"
    if (( cpus[2] == cpus[1] + 1 )); then
      PROXY_CPUSET="${cpus[1]}-${cpus[2]}"
    else
      PROXY_CPUSET="${cpus[1]},${cpus[2]}"
    fi
    BENCH_LOADGEN_CPUSET="${cpus[3]}"
  elif (( n == 2 )); then
    # Pin: backend to first cpu, proxy to second
    BACKEND_CPUSET="${cpus[0]}"
    PROXY_CPUSET="${cpus[1]}"
  else
    # Not enough cores to satisfy isolation; disable pinning explicitly
    BACKEND_CPUSET=""
    PROXY_CPUSET=""
    BENCH_LOADGEN_CPUSET=""
  fi
}


if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  setup_environment "$@"
fi

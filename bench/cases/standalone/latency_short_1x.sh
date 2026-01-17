#!/usr/bin/env bash
set -euo pipefail

# Case: latency_short_1x
# Assumptions:
# - docker and docker compose are available.
# - wrk2 is installed on the host.
# - bench/docker-compose.yaml defines bench-upstream and proxy services/ports.
# - If service names/ports differ, adjust the variables in the Config section.

CASE_NAME="latency_short_1x"
DURATION_S=30
WARMUP_S=5
COOLDOWN_S=5
CONNECTIONS=500
TARGET_RPS=10000
REQUEST_PATH="/fixed"

# Config (single place to adjust service names/ports)
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
COMPOSE_FILE="${COMPOSE_FILE:-${ROOT_DIR}/docker-compose.yaml}"
BACKEND_SERVICE="${BACKEND_SERVICE:-bench-upstream}"
BACKEND_CONTAINER="${BACKEND_CONTAINER:-bench-upstream}"
BACKEND_PORT="${BACKEND_PORT:-8001}"
PRETTY_OUTPUT="${ROOT_DIR}/scripts/pretty.sh"
if [ ! -f "$PRETTY_OUTPUT" ]; then
  echo "error: required file not found: $PRETTY_OUTPUT" >&2
  exit 1
fi
# shellcheck disable=SC1090
source "$PRETTY_OUTPUT"

PROXY="${PROXY:-pavis}"
PAVIS_PORT="${PAVIS_PORT:-8080}"
ENVOY_PORT="${ENVOY_PORT:-8081}"
NGINX_PORT="${NGINX_PORT:-8082}"
HAPROXY_PORT="${HAPROXY_PORT:-8083}"

PROXY_CPUSET_EXPECTED="${PROXY_CPUSET_EXPECTED:-1-2}"
BACKEND_CPUSET_EXPECTED="${BACKEND_CPUSET_EXPECTED:-0}"

case "$PROXY" in
  pavis)
    PROXY_SERVICE="pavis"
    PROXY_CONTAINER="bench-pavis"
    PROXY_PORT="$PAVIS_PORT"
    ;;
  envoy)
    PROXY_SERVICE="envoy"
    PROXY_CONTAINER="bench-envoy"
    PROXY_PORT="$ENVOY_PORT"
    ;;
  nginx)
    PROXY_SERVICE="nginx"
    PROXY_CONTAINER="bench-nginx"
    PROXY_PORT="$NGINX_PORT"
    ;;
  haproxy)
    PROXY_SERVICE="haproxy"
    PROXY_CONTAINER="bench-haproxy"
    PROXY_PORT="$HAPROXY_PORT"
    ;;
  *)
    echo "error: unknown PROXY '$PROXY' (expected pavis|envoy|nginx|haproxy)" >&2
    exit 1
    ;;
 esac

bench_print_case_header "$CASE_NAME" "$PROXY"

PROXY_URL="http://localhost:${PROXY_PORT}${REQUEST_PATH}"
BACKEND_URL="http://localhost:${BACKEND_PORT}/healthz"

OUTPUT_ROOT="${BENCH_OUTPUT_DIR:-${ROOT_DIR}/bench/output}"
BENCH_MODE="${BENCH_MODE:-standalone}"
RESULTS_ROOT="${OUTPUT_ROOT}/${BENCH_MODE}/${PROXY}/${CASE_NAME}${BENCH_CASE_SUFFIX:+__${BENCH_CASE_SUFFIX}}"
# TIMESTAMP removed - using simple path
BASE_DIR="${RESULTS_ROOT}"

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: missing required command '$1'" >&2
    exit 1
  }
}

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

json_string() {
  printf '"%s"' "$(json_escape "$1")"
}

json_number_or_null() {
  if [ -n "${1:-}" ]; then
    printf '%s' "$1"
  else
    printf 'null'
  fi
}

http_get() {
  local url="$1"
  if command -v curl >/dev/null 2>&1; then
    curl -fsS "$url"
  elif command -v python3 >/dev/null 2>&1; then
    python3 - <<PY
import sys, urllib.request
url = sys.argv[1]
with urllib.request.urlopen(url, timeout=2) as resp:
    sys.stdout.write(resp.read().decode())
PY
  else
    echo "error: curl or python3 required for HTTP checks" >&2
    exit 1
  fi
}

start_compose() {
  if [ "${BENCH_VERBOSE:-0}" = "1" ]; then
    docker compose -f "$COMPOSE_FILE" stop "$BACKEND_SERVICE" "$PROXY_SERVICE" >/dev/null 2>&1 || true
    docker compose -f "$COMPOSE_FILE" --profile sut up -d --force-recreate "$BACKEND_SERVICE" "$PROXY_SERVICE"
  else
    docker compose -f "$COMPOSE_FILE" stop "$BACKEND_SERVICE" "$PROXY_SERVICE" >/dev/null 2>&1 || true
    docker compose -f "$COMPOSE_FILE" --profile sut up -d --force-recreate "$BACKEND_SERVICE" "$PROXY_SERVICE" >/dev/null 2>&1
  fi
}

start_stats() {
  local outfile="$1"
  local backend="$2"
  local proxy="$3"
  echo "timestamp,container,cpu_pct,mem_usage,mem_percent" > "$outfile"
  while true; do
    local ts
    ts=$(date +%s)
    docker stats --no-stream --format "{{.Container}},{{.CPUPerc}},{{.MemUsage}},{{.MemPerc}}" "$backend" "$proxy" 2>/dev/null | while read -r line; do
      [ -n "$line" ] && echo "${ts},${line}"
    done
    sleep 1
  done >> "$outfile" &
  STATS_PID=$!
}

stop_stats() {
  if [ -n "${STATS_PID:-}" ]; then
    kill "$STATS_PID" >/dev/null 2>&1 || true
    wait "$STATS_PID" >/dev/null 2>&1 || true
  fi
}

parse_rps() {
  awk '/Requests\/sec:/ {print $2; exit}' "$1"
}

parse_errors() {
  local line
  line=$(grep -m1 "Socket errors" "$1" || true)
  if [ -z "$line" ]; then
    echo "0"
    return
  fi
  echo "$line" | sed 's/,//g' | awk '{sum=0; for(i=1;i<=NF;i++) if($i ~ /^[0-9]+$/) sum+=$i; print sum}'
}

parse_latency_pct() {
  local pct="$1"
  awk -v p="$pct" '$1==p {print $2; exit}' "$2"
}

to_ms() {
  local value="$1"
  if [ -z "$value" ]; then
    echo ""
    return
  fi
  awk -v v="$value" 'BEGIN {
    if (match(v, /^([0-9.]+)([a-zA-Z]+)$/, m)) {
      num=m[1]; unit=m[2];
      if (unit=="us") printf "%.3f", num/1000;
      else if (unit=="ms") printf "%.3f", num;
      else if (unit=="s") printf "%.3f", num*1000;
      else printf "%.3f", num;
    }
  }'
}

avg_cpu_pct() {
  local container="$1"
  awk -F, -v c="$container" '$2==c {gsub(/%/,"",$3); sum+=$3; n++} END {if(n>0) printf "%.2f", sum/n; else print "0"}' "$2"
}

peak_mem_mib() {
  local container="$1"
  awk -F, -v c="$container" '$2==c {
    split($4, parts, " /");
    val=parts[1];
    # Extract number and unit using gsub (portable across BSD and GNU awk)
    num=val; gsub(/[^0-9.]/, "", num);
    unit=val; gsub(/[0-9.]/, "", unit);
    if (unit=="MiB") mib=num;
    else if (unit=="GiB") mib=num*1024;
    else if (unit=="KiB") mib=num/1024;
    else mib=num+0;
    if (mib>max) max=mib;
  } END {if (max>0) printf "%.2f", max; else print "0"}' "$2"
}

container_image_id() {
  docker inspect -f '{{.Image}}' "$1" 2>/dev/null || echo "unknown"
}

container_image_digest() {
  local image_id
  image_id=$(container_image_id "$1")
  if [ "$image_id" = "unknown" ]; then
    echo "unknown"
    return
  fi
  local digest
  digest=$(docker inspect -f '{{index .RepoDigests 0}}' "$image_id" 2>/dev/null || true)
  if [ -z "$digest" ] || [ "$digest" = "<no value>" ]; then
    echo "unknown"
  else
    echo "$digest"
  fi
}

write_meta_json() {
  local outfile="$1"
  local backend_image_id
  local proxy_image_id
  local backend_digest
  local proxy_digest
  local kernel
  local cpu_model
  local cpu_governor
  local git_sha
  local bench_profile
  local bench_mode
  local bench_payload_size
  local bench_tls
  local bench_metrics
  local backend_cpuset
  local proxy_cpuset
  local bench_docker_compose
  local bench_host_cores
  local bench_host_cpuset_effective
  local bench_host_mem_total
  local bench_proxy_cpu_limit
  local bench_proxy_mem_limit

  backend_image_id=$(container_image_id "$BACKEND_CONTAINER")
  proxy_image_id=$(container_image_id "$PROXY_CONTAINER")
  backend_digest=$(container_image_digest "$BACKEND_CONTAINER")
  proxy_digest=$(container_image_digest "$PROXY_CONTAINER")
  kernel=$(uname -r)

  # CPU model - handle both Linux and macOS
  cpu_model="unknown"
  if [ -r /proc/cpuinfo ]; then
    cpu_model=$(awk -F: '/model name/ {print $2; exit}' /proc/cpuinfo | sed 's/^ *//')
  elif command -v sysctl >/dev/null 2>&1; then
    cpu_model=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "unknown")
  fi

  cpu_governor="unknown"
  if [ -r /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor ]; then
    cpu_governor=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)
  fi
  git_sha=$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || echo "unknown")
  bench_profile="${BENCH_PROFILE:-}"
  bench_mode="${BENCH_MODE:-}"
  bench_payload_size="${BENCH_PAYLOAD_SIZE:-}"
  bench_tls="${BENCH_TLS:-}"
  bench_metrics="${BENCH_METRICS:-}"
  backend_cpuset="${BACKEND_CPUSET:-}"
  proxy_cpuset="${PROXY_CPUSET:-}"
  bench_docker_compose="${BENCH_DOCKER_COMPOSE:-}"
  bench_host_cores="${BENCH_HOST_CORES:-}"
  bench_host_cpuset_effective="${BENCH_HOST_CPUSET_EFFECTIVE:-}"
  bench_host_mem_total="${BENCH_HOST_MEM_TOTAL:-}"
  bench_proxy_cpu_limit="${BENCH_PROXY_CPU_LIMIT:-}"
  bench_proxy_mem_limit="${BENCH_PROXY_MEM_LIMIT:-}"

  local timestamp
  timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

  cat > "$outfile" <<JSON
{
  "timestamp": $(json_string "$timestamp"),
  "case": $(json_string "$CASE_NAME"),
  "proxy": $(json_string "$PROXY"),
  "backend_container": $(json_string "$BACKEND_CONTAINER"),
  "proxy_container": $(json_string "$PROXY_CONTAINER"),
  "backend_image_id": $(json_string "$backend_image_id"),
  "proxy_image_id": $(json_string "$proxy_image_id"),
  "backend_image_digest": $(json_string "$backend_digest"),
  "proxy_image_digest": $(json_string "$proxy_digest"),
  "kernel": $(json_string "$kernel"),
  "cpu_model": $(json_string "$cpu_model"),
  "cpu_governor": $(json_string "$cpu_governor"),
  "git_sha": $(json_string "$git_sha"),
  "bench_profile": $(json_string "$bench_profile"),
  "bench_mode": $(json_string "$bench_mode"),
  "bench_payload_size": $(json_string "$bench_payload_size"),
  "bench_tls": $(json_string "$bench_tls"),
  "bench_metrics": $(json_string "$bench_metrics"),
  "backend_cpuset": $(json_string "$backend_cpuset"),
  "proxy_cpuset": $(json_string "$proxy_cpuset"),
  "bench_docker_compose": $(json_string "$bench_docker_compose"),
  "bench_host_cores": $(json_string "$bench_host_cores"),
  "bench_host_cpuset_effective": $(json_string "$bench_host_cpuset_effective"),
  "bench_host_mem_total": $(json_string "$bench_host_mem_total"),
  "bench_proxy_cpu_limit": $(json_string "$bench_proxy_cpu_limit"),
  "bench_proxy_mem_limit": $(json_string "$bench_proxy_mem_limit"),
  "target_rps": $(json_number_or_null "$TARGET_RPS")
}
JSON
}

run_loadgen() {
  local duration="$1"
  local outfile="$2"
  local taskset_cmd=()
  if [[ -n "${BENCH_LOADGEN_CPUSET:-}" ]] && command -v taskset >/dev/null 2>&1; then
    taskset_cmd=(taskset -c "$BENCH_LOADGEN_CPUSET")
  fi

  # Use bench-loadgen instead of wrk2
  # LOADGEN_BIN is exported by bench/run.sh
  if [ "${LOADGEN_WARN:-0}" = "1" ]; then
    "${taskset_cmd[@]}" "${LOADGEN_BIN}" \
      --url "$PROXY_URL" \
      --rate "$TARGET_RPS" \
      --duration "$duration" \
      --connections "$CONNECTIONS" \
      --timeout 2 \
      --output "${outfile}.json" | tee "$outfile"
  else
    "${taskset_cmd[@]}" "${LOADGEN_BIN}" \
      --url "$PROXY_URL" \
      --rate "$TARGET_RPS" \
      --duration "$duration" \
      --connections "$CONNECTIONS" \
      --timeout 2 \
      --output "${outfile}.json" 2>/dev/null | tee "$outfile" >/dev/null
  fi
}

main() {
  local start_ts
  start_ts=$(date +%s)

  require_cmd docker
  require_cmd awk

  # Check if bench-loadgen is available (built by bench/run.sh)
  if [ "${DRY_RUN:-}" != "1" ] && [ "${DRY_RUN:-}" != "true" ]; then
    if [ -z "${LOADGEN_BIN:-}" ] || [ ! -x "${LOADGEN_BIN}" ]; then
      echo "error: bench-loadgen not found at ${LOADGEN_BIN:-not set}" >&2
      echo "Please run this script via bench/run.sh or build manually:" >&2
      echo "  cargo build -p pavis-benchkit --bin bench-loadgen --release" >&2
      exit 1
    fi
  fi

  mkdir -p "$BASE_DIR"

  start_compose

  http_get "$BACKEND_URL" >/dev/null || {
    bench_print_backend_status "fail"
    exit 1
  }
  bench_print_backend_status "ok"

  bench_print_tool_info "loadgen" "$DURATION_S" "$CONNECTIONS" "$TARGET_RPS"

  # Check if DRY_RUN mode is enabled
  if [ "${DRY_RUN:-}" = "1" ] || [ "${DRY_RUN:-}" = "true" ]; then
    bench_print_metric "💤" "Dry-run" "Setup validated; benchmark skipped"
    local end_ts
    end_ts=$(date +%s)
    bench_print_duration $((end_ts - start_ts))
    return 0
  fi

  write_meta_json "$BASE_DIR/meta.json"

  local run_dir="$BASE_DIR"
  local loadgen_out="${run_dir}/loadgen.txt"

  # Warmup run
  run_loadgen "$WARMUP_S" "${run_dir}/warmup.txt" >/dev/null || true
  sleep "$COOLDOWN_S"

  # Main benchmark run
  start_stats "${run_dir}/docker_stats.csv" "$BACKEND_CONTAINER" "$PROXY_CONTAINER"
  run_loadgen "$DURATION_S" "$loadgen_out"
  stop_stats

  # Parse bench-loadgen JSON output
  local loadgen_json="${loadgen_out}.json"

  # Extract metrics from bench-loadgen JSON output
  local achieved_rps
  local errors
  local p50
  local p99
  local dropped
  local backend_cpu
  local proxy_cpu
  local peak_mem

  achieved_rps=$(jq -r '.achieved_rps' "$loadgen_json")
  errors=$(jq -r '.errors' "$loadgen_json")
  p50=$(jq -r '.latency_ms.p50' "$loadgen_json")
  p99=$(jq -r '.latency_ms.p99' "$loadgen_json")
  dropped=$(jq -r '.dropped' "$loadgen_json")
  backend_cpu=$(avg_cpu_pct "$BACKEND_CONTAINER" "${run_dir}/docker_stats.csv")
  proxy_cpu=$(avg_cpu_pct "$PROXY_CONTAINER" "${run_dir}/docker_stats.csv")
  peak_mem=$(peak_mem_mib "$PROXY_CONTAINER" "${run_dir}/docker_stats.csv")

  bench_print_metric "🎯" "Target RPS" "$TARGET_RPS"
  bench_print_metric "📊" "Achieved RPS" "$achieved_rps"
  bench_print_metric "⏱️" "Latency" "p50=${p50}ms · p99=${p99}ms"
  bench_print_metric "🖥️" "Backend CPU (%)" "${backend_cpu}"
  bench_print_metric "💻" "Proxy CPU (%)" "${proxy_cpu}"
  bench_print_metric "🧠" "Proxy peak RSS (MiB)" "${peak_mem}"
  bench_print_errors_line "$errors"
  bench_print_dropped_line "$dropped"
  bench_print_completion "$errors" "$dropped"
  local end_ts
  end_ts=$(date +%s)
  bench_print_duration $((end_ts - start_ts))

  # Raw outputs kept: loadgen.txt.json, docker_stats.csv, meta.json
}

trap stop_stats EXIT
main "$@"

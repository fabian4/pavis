#!/usr/bin/env bash
set -euo pipefail

# Summarize benchmark results from raw outputs
# Scans bench/output/{mode}/{proxy}/{case}/ directories and aggregates into summary.csv
#
# Usage:
#   bash bench/scripts/summarize_github.sh [output_dir]
#
# Environment:
#   OUTPUT_DIR: Override default bench/output/standalone directory
#   PROFILE_FILTER: Optional bench_profile filter (e.g. github or workstation)

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_LIB_DIR="$ROOT_DIR/../scripts/lib"
source "$SCRIPT_LIB_DIR/json.sh"

OUTPUT_DIR="${1:-${OUTPUT_DIR:-${ROOT_DIR}/output/standalone}}"
SUMMARY_CSV="${OUTPUT_DIR}/summary.csv"
MODE_FILTER="standalone"
PROFILE_FILTER="${PROFILE_FILTER:-}"
PROFILE_DETECTED=""
PROFILE_MIXED=0

# Helper functions
parse_wrk_rps() {
  awk '/Requests\/sec:/ {print $2; exit}' "$1"
}

parse_wrk_latency_pct() {
  local pct="$1"
  local file="$2"
  awk -v p="$pct" '$1==p {print $2; exit}' "$file"
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

parse_wrk_errors() {
  local line
  line=$(grep -m1 "Socket errors" "$1" || true)
  if [ -z "$line" ]; then
    echo "0"
    return
  fi
  echo "$line" | sed 's/,//g' | awk '{sum=0; for(i=1;i<=NF;i++) if($i ~ /^[0-9]+$/) sum+=$i; print sum}'
}

is_number() {
  local value="$1"
  [[ "$value" =~ ^-?[0-9]+([.][0-9]+)?([eE][-+]?[0-9]+)?$ ]]
}

csv_escape() {
  local value="$1"
  value=${value//\"/\"\"}
  printf '"%s"' "$value"
}

csv_field() {
  local value="$1"
  if [ -z "$value" ]; then
    echo ""
    return
  fi
  if is_number "$value"; then
    echo "$value"
    return
  fi
  csv_escape "$value"
}

median_value() {
  local file="$1"
  local fmt="${2:-%.3f}"
  if [ ! -s "$file" ]; then
    echo ""
    return
  fi
  sort -n "$file" | awk -v fmt="$fmt" '{vals[NR]=$1} END {
    n=NR;
    if (n==0) { print ""; exit }
    if (n%2==1) med=vals[(n+1)/2]; else med=(vals[n/2]+vals[n/2+1])/2;
    printf fmt, med
  }'
}

median_iqr() {
  local file="$1"
  local fmt="${2:-%.3f}"
  if [ ! -s "$file" ]; then
    echo ""
    return
  fi
  sort -n "$file" | awk -v fmt="$fmt" '{vals[NR]=$1} END {
    n=NR;
    if (n==0) { print ""; exit }
    if (n%2==1) med=vals[(n+1)/2]; else med=(vals[n/2]+vals[n/2+1])/2;
    if (n>=4) {
      q1_pos=int(n/4+0.5); if (q1_pos<1) q1_pos=1;
      q3_pos=int(3*n/4+0.5); if (q3_pos>n) q3_pos=n;
      iqr=vals[q3_pos]-vals[q1_pos];
    } else if (n==3) {
      iqr=vals[3]-vals[1];
    } else if (n==2) {
      iqr=vals[2]-vals[1];
    } else {
      iqr=0;
    }
    printf fmt " " fmt, med, iqr
  }'
}

max_value() {
  local file="$1"
  local fmt="${2:-%.2f}"
  if [ ! -s "$file" ]; then
    echo ""
    return
  fi
  sort -n "$file" | awk -v fmt="$fmt" 'END {
    if (NR>0) { printf fmt, $1 }
  }'
}

# Extract CPU/memory metrics from docker_stats.csv
avg_cpu_pct() {
  local container="$1"
  local stats_file="$2"
  if [ ! -f "$stats_file" ]; then
    echo ""
    return
  fi
  awk -F, -v c="$container" '$2==c {gsub(/%/,"",$3); sum+=$3; n++} END {if(n>0) printf "%.2f", sum/n; else print ""}' "$stats_file"
}

peak_mem_mib() {
  local container="$1"
  local stats_file="$2"
  if [ ! -f "$stats_file" ]; then
    echo ""
    return
  fi
  awk -F, -v c="$container" '$2==c {
    split($4, parts, " /");
    val=parts[1];
    num=val; gsub(/[^0-9.]/, "", num);
    unit=val; gsub(/[0-9.]/, "", unit);
    if (unit=="MiB") mib=num;
    else if (unit=="GiB") mib=num*1024;
    else if (unit=="KiB") mib=num/1024;
    else mib=num+0;
    if (mib>max) max=mib;
  } END {if (max>0) printf "%.2f", max; else print ""}' "$stats_file"
}

emit_row() {
  local git_sha="$1"
  local iteration="$2"
  local aggregate="$3"
  local phase="$4"
  local proxy="$5"
  local case_name="$6"
  local case_type="$7"
  local runs="$8"
  local achieved_rps="$9"
  local p50_ms="${10}"
  local p90_ms="${11}"
  local p99_ms="${12}"
  local errors="${13}"
  local dropped="${14}"
  local rps_iqr="${15}"
  local p99_iqr="${16}"
  local backend_cpu="${17}"
  local proxy_cpu="${18}"
  local peak_mem="${19}"
  local target_rps="${20}"
  local timestamp="${21}"
  local cpu_model="${22}"
  local kernel="${23}"
  local bench_profile="${24}"
  local bench_mode="${25}"
  local bench_payload_size="${26}"
  local bench_tls="${27}"
  local bench_metrics="${28}"
  local backend_cpuset="${29}"
  local proxy_cpuset="${30}"
  local bench_docker_compose="${31}"
  local bench_host_cores="${32}"
  local bench_host_cpuset_effective="${33}"
  local bench_host_mem_total="${34}"
  local bench_proxy_cpu_limit="${35}"
  local bench_proxy_mem_limit="${36}"

  echo "$(csv_field "$git_sha"),$(csv_field "$iteration"),$(csv_field "$aggregate"),$(csv_field "$phase"),$(csv_field "$proxy"),$(csv_field "$case_name"),$(csv_field "$case_type"),$(csv_field "$runs"),$(csv_field "$achieved_rps"),$(csv_field "$p50_ms"),$(csv_field "$p90_ms"),$(csv_field "$p99_ms"),$(csv_field "$errors"),$(csv_field "$dropped"),$(csv_field "$rps_iqr"),$(csv_field "$p99_iqr"),$(csv_field "$backend_cpu"),$(csv_field "$proxy_cpu"),$(csv_field "$peak_mem"),$(csv_field "$target_rps"),$(csv_field "$timestamp"),$(csv_field "$cpu_model"),$(csv_field "$kernel"),$(csv_field "$bench_profile"),$(csv_field "$bench_mode"),$(csv_field "$bench_payload_size"),$(csv_field "$bench_tls"),$(csv_field "$bench_metrics"),$(csv_field "$backend_cpuset"),$(csv_field "$proxy_cpuset"),$(csv_field "$bench_docker_compose"),$(csv_field "$bench_host_cores"),$(csv_field "$bench_host_cpuset_effective"),$(csv_field "$bench_host_mem_total"),$(csv_field "$bench_proxy_cpu_limit"),$(csv_field "$bench_proxy_mem_limit")"
}

# Parse single test case
parse_case() {
  local proxy="$1"
  local case_name="$2"
  local case_dir="$3"
  local phase="$4"

  local case_type=""
  local achieved_rps=""
  local p50_ms=""
  local p90_ms=""
  local p99_ms=""
  local errors=""
  local dropped=""
  local run_count="1"
  local rps_iqr=""
  local p99_iqr=""
  local backend_cpu=""
  local proxy_cpu=""
  local peak_mem=""
  local timestamp=""
  local git_sha=""
  local target_rps=""
  local cpu_model=""
  local kernel=""
  local bench_profile=""
  local bench_mode=""
  local bench_payload_size=""
  local bench_tls=""
  local bench_metrics=""
  local backend_cpuset=""
  local proxy_cpuset=""
  local bench_docker_compose=""
  local bench_host_cores=""
  local bench_host_cpuset_effective=""
  local bench_host_mem_total=""
  local bench_proxy_cpu_limit=""
  local bench_proxy_mem_limit=""
  local backend_container=""
  local proxy_container=""

  # Extract meta information early for iteration rows
  # Prefer run-level context.env; legacy case-level context.env is supported.
  local run_context="${case_dir%/*}/context.env"
  if [ -f "${case_dir}/context.env" ]; then
    # shellcheck source=/dev/null
    source "${case_dir}/context.env"
  elif [ -f "$run_context" ]; then
    # shellcheck source=/dev/null
    source "$run_context"
  else
    echo "error: missing run-level context.env in ${case_dir} (required for profile/mode filtering)" >&2
    return 1
  fi

  if [ -n "${RUN_TIMESTAMP:-}" ] || [ -n "${GIT_SHA:-}" ]; then
    timestamp="${RUN_TIMESTAMP:-}"
    git_sha="${GIT_SHA:-}"
    cpu_model="${BENCH_HOST_CPU_MODEL:-}"
    kernel="${BENCH_HOST_KERNEL:-}"
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
  fi
  # Note: backend_container and proxy_container not in context.env, read from meta.json if needed
  if [ -f "${case_dir}/meta.json" ]; then
    backend_container=$(jq -r '.backend_container // empty' "${case_dir}/meta.json" 2>/dev/null || true)
    proxy_container=$(jq -r '.proxy_container // empty' "${case_dir}/meta.json" 2>/dev/null || true)
    target_rps=$(jq -r '.target_rps // empty' "${case_dir}/meta.json" 2>/dev/null || true)
  fi

  if [ "$bench_mode" != "$MODE_FILTER" ]; then
    return 0
  fi
  if [ -z "$PROFILE_DETECTED" ]; then
    PROFILE_DETECTED="$bench_profile"
  elif [ "$bench_profile" != "$PROFILE_DETECTED" ]; then
    PROFILE_MIXED=1
  fi
  if [ -n "$PROFILE_FILTER" ] && [ "$bench_profile" != "$PROFILE_FILTER" ]; then
    return 0
  fi

  # Determine test type by checking which files exist
  if [ -f "${case_dir}/wrk.txt" ]; then
    # wrk-based test (throughput, concurrency, churn)
    case_type="wrk"
    achieved_rps=$(parse_wrk_rps "${case_dir}/wrk.txt")
    p50_ms=$(to_ms "$(parse_wrk_latency_pct "50%" "${case_dir}/wrk.txt")")
    p90_ms=$(to_ms "$(parse_wrk_latency_pct "90%" "${case_dir}/wrk.txt")")
    p99_ms=$(to_ms "$(parse_wrk_latency_pct "99%" "${case_dir}/wrk.txt")")
    errors=$(parse_wrk_errors "${case_dir}/wrk.txt")
    dropped=""

  elif [ -f "${case_dir}/loadgen.txt.json" ]; then
    # loadgen single-run test (latency_short_1x)
    case_type="loadgen-single"
    achieved_rps=$(jq -r '.achieved_rps' "${case_dir}/loadgen.txt.json")
    p50_ms=$(jq -r '.latency_ms.p50' "${case_dir}/loadgen.txt.json")
    p90_ms=$(jq -r '.latency_ms.p90' "${case_dir}/loadgen.txt.json")
    p99_ms=$(jq -r '.latency_ms.p99' "${case_dir}/loadgen.txt.json")
    errors=$(jq -r '.errors' "${case_dir}/loadgen.txt.json")
    dropped=$(jq -r '.dropped' "${case_dir}/loadgen.txt.json")

  elif [ -d "${case_dir}/run_1" ]; then
    # Multi-run test (Detect by presence of run_*/ subdirectories)
    # Could be loadgen-multi (result.json) or wrk-multi (wrk.txt)

    # Count runs and collect metrics
    local rps_temp="${case_dir}/.rps_temp.txt"
    local p50_temp="${case_dir}/.p50_temp.txt"
    local p90_temp="${case_dir}/.p90_temp.txt"
    local p99_temp="${case_dir}/.p99_temp.txt"
    local backend_cpu_temp="${case_dir}/.backend_cpu_temp.txt"
    local proxy_cpu_temp="${case_dir}/.proxy_cpu_temp.txt"
    local peak_mem_temp="${case_dir}/.peak_mem_temp.txt"
    local errors_sum=0
    local dropped_sum=0
    : > "$rps_temp"
    : > "$p50_temp"
    : > "$p90_temp"
    : > "$p99_temp"
    : > "$backend_cpu_temp"
    : > "$proxy_cpu_temp"
    : > "$peak_mem_temp"

    run_count=0

    for run_dir in "${case_dir}"/run_*; do
      if [ -d "$run_dir" ]; then
        local run_rps=""
        local run_p50=""
        local run_p90=""
        local run_p99=""
        local run_errors=0
        local run_dropped=0

        if [ -f "${run_dir}/result.json" ]; then
          case_type="loadgen-multi"
          run_rps=$(jq -r '.achieved_rps // empty' "${run_dir}/result.json")
          run_p50=$(jq -r '.latency_ms.p50 // empty' "${run_dir}/result.json")
          run_p90=$(jq -r '.latency_ms.p90 // empty' "${run_dir}/result.json")
          run_p99=$(jq -r '.latency_ms.p99 // empty' "${run_dir}/result.json")
          run_errors=$(jq -r '.errors // 0' "${run_dir}/result.json")
          run_dropped=$(jq -r '.dropped // 0' "${run_dir}/result.json")
        elif [ -f "${run_dir}/wrk.txt" ]; then
          case_type="wrk-multi"
          run_rps=$(parse_wrk_rps "${run_dir}/wrk.txt")
          run_p50=$(to_ms "$(parse_wrk_latency_pct "50%" "${run_dir}/wrk.txt")")
          run_p90=$(to_ms "$(parse_wrk_latency_pct "90%" "${run_dir}/wrk.txt")")
          run_p99=$(to_ms "$(parse_wrk_latency_pct "99%" "${run_dir}/wrk.txt")")
          run_errors=$(parse_wrk_errors "${run_dir}/wrk.txt")
          run_dropped=""
        else
          continue
        fi

        run_count=$((run_count + 1))

        local run_backend_cpu=""
        local run_proxy_cpu=""
        local run_peak_mem=""
        if [ -f "${run_dir}/docker_stats.csv" ]; then
          if [ -n "$backend_container" ]; then
            run_backend_cpu=$(avg_cpu_pct "$backend_container" "${run_dir}/docker_stats.csv")
          fi
          if [ -n "$proxy_container" ]; then
            run_proxy_cpu=$(avg_cpu_pct "$proxy_container" "${run_dir}/docker_stats.csv")
            run_peak_mem=$(peak_mem_mib "$proxy_container" "${run_dir}/docker_stats.csv")
          fi
        fi

        emit_row "$git_sha" "$run_count" "0" "$phase" \
          "$proxy" "$case_name" "$case_type" "" \
          "$run_rps" "$run_p50" "$run_p90" "$run_p99" "$run_errors" "$run_dropped" \
          "" "" "$run_backend_cpu" "$run_proxy_cpu" "$run_peak_mem" \
          "$target_rps" "$timestamp" "$cpu_model" "$kernel" \
          "$bench_profile" "$bench_mode" "$bench_payload_size" "$bench_tls" "$bench_metrics" \
          "$backend_cpuset" "$proxy_cpuset" "$bench_docker_compose" \
          "$bench_host_cores" "$bench_host_cpuset_effective" "$bench_host_mem_total" "$bench_proxy_cpu_limit" "$bench_proxy_mem_limit"

        [ -n "$run_rps" ] && echo "$run_rps" >> "$rps_temp"
        [ -n "$run_p50" ] && echo "$run_p50" >> "$p50_temp"
        [ -n "$run_p90" ] && echo "$run_p90" >> "$p90_temp"
        [ -n "$run_p99" ] && echo "$run_p99" >> "$p99_temp"
        [ -n "$run_backend_cpu" ] && echo "$run_backend_cpu" >> "$backend_cpu_temp"
        [ -n "$run_proxy_cpu" ] && echo "$run_proxy_cpu" >> "$proxy_cpu_temp"
        [ -n "$run_peak_mem" ] && echo "$run_peak_mem" >> "$peak_mem_temp"
        errors_sum=$((errors_sum + ${run_errors:-0}))
        dropped_sum=$((dropped_sum + ${run_dropped:-0}))
      fi
    done

    # Calculate median and IQR
    if [ -s "$rps_temp" ]; then
      local rps_result
      rps_result=$(median_iqr "$rps_temp" "%.3f")
      read -r achieved_rps rps_iqr <<< "$rps_result"
    fi

    if [ -s "$p99_temp" ]; then
      local p99_result
      p99_result=$(median_iqr "$p99_temp" "%.3f")
      read -r p99_ms p99_iqr <<< "$p99_result"
    fi

    p50_ms=$(median_value "$p50_temp" "%.3f")
    p90_ms=$(median_value "$p90_temp" "%.3f")
    backend_cpu=$(median_value "$backend_cpu_temp" "%.2f")
    proxy_cpu=$(median_value "$proxy_cpu_temp" "%.2f")
    peak_mem=$(max_value "$peak_mem_temp" "%.2f")
    errors="$errors_sum"
    dropped="$dropped_sum"

    rm -f "$rps_temp" "$p50_temp" "$p90_temp" "$p99_temp" \
      "$backend_cpu_temp" "$proxy_cpu_temp" "$peak_mem_temp"

  else
    echo "warn: cannot determine test type for ${case_dir}" >&2
    return
  fi

  # Extract CPU and memory metrics from docker_stats.csv
  local stats_csv="${case_dir}/docker_stats.csv"
  if [ -f "$stats_csv" ]; then
    # Extract container names from meta.json
    if [ -f "${case_dir}/meta.json" ]; then
      if [ -n "$backend_container" ]; then
        backend_cpu=$(avg_cpu_pct "$backend_container" "$stats_csv")
      fi
      if [ -n "$proxy_container" ]; then
        proxy_cpu=$(avg_cpu_pct "$proxy_container" "$stats_csv")
        peak_mem=$(peak_mem_mib "$proxy_container" "$stats_csv")
      fi
    fi
  fi

  if [ "$case_type" = "loadgen-multi" ] || [ "$case_type" = "wrk-multi" ]; then
    emit_row "$git_sha" "0" "1" "$phase" \
      "$proxy" "$case_name" "$case_type" "$run_count" \
      "$achieved_rps" "$p50_ms" "$p90_ms" "$p99_ms" "$errors" "$dropped" \
      "$rps_iqr" "$p99_iqr" "$backend_cpu" "$proxy_cpu" "$peak_mem" \
      "$target_rps" "$timestamp" "$cpu_model" "$kernel" \
      "$bench_profile" "$bench_mode" "$bench_payload_size" "$bench_tls" "$bench_metrics" \
      "$backend_cpuset" "$proxy_cpuset" "$bench_docker_compose" \
      "$bench_host_cores" "$bench_host_cpuset_effective" "$bench_host_mem_total" "$bench_proxy_cpu_limit" "$bench_proxy_mem_limit"
  else
    emit_row "$git_sha" "1" "1" "$phase" \
      "$proxy" "$case_name" "$case_type" "1" \
      "$achieved_rps" "$p50_ms" "$p90_ms" "$p99_ms" "$errors" "$dropped" \
      "" "" "$backend_cpu" "$proxy_cpu" "$peak_mem" \
      "$target_rps" "$timestamp" "$cpu_model" "$kernel" \
      "$bench_profile" "$bench_mode" "$bench_payload_size" "$bench_tls" "$bench_metrics" \
      "$backend_cpuset" "$proxy_cpuset" "$bench_docker_compose" \
      "$bench_host_cores" "$bench_host_cpuset_effective" "$bench_host_mem_total" "$bench_proxy_cpu_limit" "$bench_proxy_mem_limit"
  fi
}

main() {
  if [ ! -d "$OUTPUT_DIR" ]; then
    echo "error: output directory not found: $OUTPUT_DIR" >&2
    echo "Run benchmarks first: MODE=standalone BENCH_PROFILE=github make bench" >&2
    exit 1
  fi

  if [ -d "${OUTPUT_DIR}/standalone" ] || [ -d "${OUTPUT_DIR}/system" ]; then
    echo "error: output directory should be a mode root like bench/output/standalone" >&2
    exit 1
  fi

  local run_context=""
  for proxy_dir in "$OUTPUT_DIR"/*; do
    if [ -f "${proxy_dir}/context.env" ]; then
      if [ "$(basename "$proxy_dir")" != "pavis" ]; then
        continue
      fi
      run_context="${proxy_dir}/context.env"
      break
    fi
  done
  if [ -n "$run_context" ]; then
    # shellcheck source=/dev/null
    source "$run_context"
    if [ "${BENCH_MODE:-}" = "system" ]; then
      echo "warn: summarize_github.sh does not support system mode outputs; skipping" >&2
      exit 0
    fi
  fi

  local phase="measure"

  # CSV header
  echo "git_sha,iteration,aggregate,phase,proxy,case,type,runs,achieved_rps,p50_ms,p90_ms,p99_ms,errors,dropped,rps_iqr,p99_iqr,backend_cpu,proxy_cpu_avg,proxy_mem_peak_mib,target_rps,timestamp,cpu_model,kernel,bench_profile,bench_mode,bench_payload_size,bench_tls,bench_metrics,backend_cpuset,proxy_cpuset,bench_docker_compose,bench_host_cores,bench_host_cpuset_effective,bench_host_mem_total,bench_proxy_cpu_limit,bench_proxy_mem_limit" > "$SUMMARY_CSV"

  # Scan all proxy/case directories
  local found_results=0
  for proxy_dir in "$OUTPUT_DIR"/*; do
    if [ ! -d "$proxy_dir" ]; then
      continue
    fi

    local proxy
    proxy=$(basename "$proxy_dir")
    if [ "$proxy" != "pavis" ]; then
      continue
    fi

    for case_dir in "$proxy_dir"/*; do
      if [ ! -d "$case_dir" ]; then
        continue
      fi

      local case_name
      case_name=$(basename "$case_dir")
      case_name="${case_name%%__*}"

      parse_case "$proxy" "$case_name" "$case_dir" "$phase" >> "$SUMMARY_CSV"
      found_results=1
    done
  done

  if [ "$found_results" = "0" ]; then
    echo "warn: no benchmark results found in $OUTPUT_DIR" >&2
    echo "Run benchmarks first: MODE=standalone BENCH_PROFILE=github make bench" >&2
    exit 1
  fi

  if [ "$PROFILE_MIXED" = "1" ]; then
    echo "error: mixed bench_profile values detected in $OUTPUT_DIR" >&2
    exit 1
  fi

  if [ "$(wc -l < "$SUMMARY_CSV")" -le 1 ]; then
    echo "error: no standalone rows found in $OUTPUT_DIR" >&2
    exit 1
  fi

  echo "Summary written to $SUMMARY_CSV"
}

main "$@"

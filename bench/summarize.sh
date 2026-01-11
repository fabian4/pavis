#!/usr/bin/env bash
set -euo pipefail

# Summarize benchmark results from raw outputs
# Scans bench/output/{proxy}/{case}/ directories and aggregates into summary.csv
#
# Usage:
#   bash bench/summarize.sh [output_dir]
#
# Environment:
#   OUTPUT_DIR: Override default bench/output directory

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${1:-${OUTPUT_DIR:-${ROOT_DIR}/bench/output}}"
SUMMARY_CSV="${OUTPUT_DIR}/summary.csv"

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

# Parse single test case
parse_case() {
  local proxy="$1"
  local case_name="$2"
  local case_dir="$3"

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

  elif [ -f "${case_dir}/aggregate.json" ]; then
    # loadgen multi-run test (reload_short_1x, latency_extended_1x)
    case_type="loadgen-multi"
    run_count=$(jq -r '.run_count' "${case_dir}/aggregate.json")
    achieved_rps=$(jq -r '.rps_median' "${case_dir}/aggregate.json")
    rps_iqr=$(jq -r '.rps_iqr' "${case_dir}/aggregate.json")
    p99_ms=$(jq -r '.p99_median' "${case_dir}/aggregate.json")
    p99_iqr=$(jq -r '.p99_iqr' "${case_dir}/aggregate.json")
    p50_ms=""
    p90_ms=""
    errors="0"
    dropped=""

  else
    echo "warn: cannot determine test type for ${case_dir}" >&2
    return
  fi

  # Output CSV row
  echo "${proxy},${case_name},${case_type},${run_count},${achieved_rps},${p50_ms},${p90_ms},${p99_ms},${errors},${dropped},${rps_iqr},${p99_iqr}"
}

main() {
  if [ ! -d "$OUTPUT_DIR" ]; then
    echo "error: output directory not found: $OUTPUT_DIR" >&2
    echo "Run benchmarks first: make bench" >&2
    exit 1
  fi

  # CSV header
  echo "proxy,case,type,runs,achieved_rps,p50_ms,p90_ms,p99_ms,errors,dropped,rps_iqr,p99_iqr" > "$SUMMARY_CSV"

  # Scan all proxy/case directories
  local found_results=0
  for proxy_dir in "$OUTPUT_DIR"/*; do
    if [ ! -d "$proxy_dir" ]; then
      continue
    fi

    local proxy
    proxy=$(basename "$proxy_dir")

    for case_dir in "$proxy_dir"/*; do
      if [ ! -d "$case_dir" ]; then
        continue
      fi

      local case_name
      case_name=$(basename "$case_dir")

      parse_case "$proxy" "$case_name" "$case_dir" >> "$SUMMARY_CSV"
      found_results=1
    done
  done

  if [ "$found_results" = "0" ]; then
    echo "warn: no benchmark results found in $OUTPUT_DIR" >&2
    echo "Run benchmarks first: make bench" >&2
    exit 1
  fi

  echo "Summary written to $SUMMARY_CSV"
  echo ""
  echo "Results:"
  column -t -s, "$SUMMARY_CSV"
}

main "$@"

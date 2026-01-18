#!/usr/bin/env bash
set -euo pipefail

# Summarize system-mode benchmark results from metrics.json outputs.
#
# Usage:
#   bash bench/scripts/summarize_system.sh [output_dir]
#
# Environment:
#   OUTPUT_DIR: Override default bench/output/system directory

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_LIB_DIR="$ROOT_DIR/../scripts/lib"
source "$SCRIPT_LIB_DIR/json.sh"

OUTPUT_DIR="${1:-${OUTPUT_DIR:-${ROOT_DIR}/output/system}}"
SUMMARY_CSV="${OUTPUT_DIR}/summary.csv"

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

emit_row() {
  local git_sha="$1"
  local timestamp="$2"
  local proxy="$3"
  local case_name="$4"
  local target_rps="$5"
  local baseline_p99_ms="$6"
  local stress_p99_ms="$7"
  local recovery_p99_ms="$8"
  local transition_p99_ms="$9"
  local convergence_time_ms="${10}"
  local rollback_ttbr_ms="${11}"
  local p99_delta_ms="${12}"
  local latency_recovery_pct="${13}"
  local stress_dropped="${14}"
  local errors="${15}"
  local errors_5xx="${16}"
  local p99_ms="${17}"
  local baseline_rps="${18}"
  local stress_rps="${19}"
  local baseline_achieved_rps="${20}"
  local stress_achieved_rps="${21}"
  local recovery_achieved_rps="${22}"
  local baseline_rss_kb="${23}"
  local stress_rss_peak_kb="${24}"
  local recovery_rss_kb="${25}"
  local rss_growth_mb="${26}"
  local rss_growth_pct="${27}"
  local config_version_before="${28}"
  local config_version_after="${29}"
  local duration_s="${30}"
  local achieved_rps="${31}"
  local baseline_restored="${32}"
  local config_versions="${33}"
  local degraded_achieved_rps="${34}"
  local degraded_errors="${35}"
  local recovery_errors="${36}"
  local bench_profile="${37}"
  local bench_mode="${38}"

  echo "$(csv_field "$git_sha"),$(csv_field "$timestamp"),$(csv_field "$proxy"),$(csv_field "$case_name"),$(csv_field "$target_rps"),$(csv_field "$baseline_rps"),$(csv_field "$stress_rps"),$(csv_field "$baseline_achieved_rps"),$(csv_field "$stress_achieved_rps"),$(csv_field "$recovery_achieved_rps"),$(csv_field "$baseline_p99_ms"),$(csv_field "$stress_p99_ms"),$(csv_field "$recovery_p99_ms"),$(csv_field "$transition_p99_ms"),$(csv_field "$p99_delta_ms"),$(csv_field "$convergence_time_ms"),$(csv_field "$rollback_ttbr_ms"),$(csv_field "$latency_recovery_pct"),$(csv_field "$stress_dropped"),$(csv_field "$errors"),$(csv_field "$errors_5xx"),$(csv_field "$p99_ms"),$(csv_field "$baseline_rss_kb"),$(csv_field "$stress_rss_peak_kb"),$(csv_field "$recovery_rss_kb"),$(csv_field "$rss_growth_mb"),$(csv_field "$rss_growth_pct"),$(csv_field "$config_version_before"),$(csv_field "$config_version_after"),$(csv_field "$duration_s"),$(csv_field "$achieved_rps"),$(csv_field "$baseline_restored"),$(csv_field "$config_versions"),$(csv_field "$degraded_achieved_rps"),$(csv_field "$degraded_errors"),$(csv_field "$recovery_errors"),$(csv_field "$bench_profile"),$(csv_field "$bench_mode")"
}

main() {
  if [ ! -d "$OUTPUT_DIR" ]; then
    echo "warn: system output directory not found: $OUTPUT_DIR" >&2
    exit 0
  fi

  local run_context="${OUTPUT_DIR}/context.env"
  if [ ! -f "$run_context" ]; then
    echo "warn: missing run-level context.env in $OUTPUT_DIR" >&2
    exit 0
  fi

  # shellcheck source=/dev/null
  source "$run_context"

  if [ "${BENCH_MODE:-}" != "system" ]; then
    echo "warn: summary expects system mode, got ${BENCH_MODE:-unknown}" >&2
    exit 0
  fi

  echo "git_sha,timestamp,proxy,case,target_rps,baseline_rps,stress_rps,baseline_achieved_rps,stress_achieved_rps,recovery_achieved_rps,baseline_p99_ms,stress_p99_ms,recovery_p99_ms,transition_p99_ms,p99_delta_ms,convergence_time_ms,rollback_ttbr_ms,latency_recovery_pct,stress_dropped,errors,errors_5xx,p99_ms,baseline_rss_kb,stress_rss_peak_kb,recovery_rss_kb,rss_growth_mb,rss_growth_pct,config_version_before,config_version_after,duration_s,achieved_rps,baseline_restored,config_versions,degraded_achieved_rps,degraded_errors,recovery_errors,bench_profile,bench_mode" > "$SUMMARY_CSV"

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
      case_name="${case_name%%__*}"

      local metrics="${case_dir}/metrics.json"
      if [ ! -f "$metrics" ]; then
        continue
      fi

      found_results=1

      local target_rps
      target_rps=$(jq -r '.target_rps // empty' "$metrics" 2>/dev/null || true)
      local baseline_p99_ms
      baseline_p99_ms=$(jq -r '.baseline_p99_ms // empty' "$metrics" 2>/dev/null || true)
      local stress_p99_ms
      stress_p99_ms=$(jq -r '.stress_p99_ms // empty' "$metrics" 2>/dev/null || true)
      local recovery_p99_ms
      recovery_p99_ms=$(jq -r '.recovery_p99_ms // empty' "$metrics" 2>/dev/null || true)
      local transition_p99_ms
      transition_p99_ms=$(jq -r '.transition_p99_ms // empty' "$metrics" 2>/dev/null || true)
      local p99_delta_ms
      p99_delta_ms=$(jq -r '.p99_delta_ms // empty' "$metrics" 2>/dev/null || true)
      local convergence_time_ms
      convergence_time_ms=$(jq -r '.convergence_time_ms // empty' "$metrics" 2>/dev/null || true)
      local rollback_ttbr_ms
      rollback_ttbr_ms=$(jq -r '.rollback_ttbr_ms // empty' "$metrics" 2>/dev/null || true)
      local latency_recovery_pct
      latency_recovery_pct=$(jq -r '.latency_recovery_pct // empty' "$metrics" 2>/dev/null || true)
      local errors
      errors=$(jq -r '.errors // empty' "$metrics" 2>/dev/null || true)
      local errors_5xx
      errors_5xx=$(jq -r '.errors_5xx // empty' "$metrics" 2>/dev/null || true)
      local stress_dropped
      stress_dropped=$(jq -r '.stress_dropped // empty' "$metrics" 2>/dev/null || true)
      local baseline_rps
      baseline_rps=$(jq -r '.baseline_rps // empty' "$metrics" 2>/dev/null || true)
      local stress_rps
      stress_rps=$(jq -r '.stress_rps // empty' "$metrics" 2>/dev/null || true)
      local baseline_achieved_rps
      baseline_achieved_rps=$(jq -r '.baseline_achieved_rps // empty' "$metrics" 2>/dev/null || true)
      local stress_achieved_rps
      stress_achieved_rps=$(jq -r '.stress_achieved_rps // empty' "$metrics" 2>/dev/null || true)
      local recovery_achieved_rps
      recovery_achieved_rps=$(jq -r '.recovery_achieved_rps // empty' "$metrics" 2>/dev/null || true)
      local achieved_rps
      achieved_rps=$(jq -r '.achieved_rps // empty' "$metrics" 2>/dev/null || true)
      local p99_ms
      p99_ms=$(jq -r '.p99_ms // empty' "$metrics" 2>/dev/null || true)
      local baseline_rss_kb
      baseline_rss_kb=$(jq -r '.baseline_rss_kb // empty' "$metrics" 2>/dev/null || true)
      local stress_rss_peak_kb
      stress_rss_peak_kb=$(jq -r '.stress_rss_peak_kb // empty' "$metrics" 2>/dev/null || true)
      local recovery_rss_kb
      recovery_rss_kb=$(jq -r '.recovery_rss_kb // empty' "$metrics" 2>/dev/null || true)
      local rss_growth_mb
      rss_growth_mb=$(jq -r '.rss_growth_mb // empty' "$metrics" 2>/dev/null || true)
      local rss_growth_pct
      rss_growth_pct=$(jq -r '.rss_growth_pct // empty' "$metrics" 2>/dev/null || true)
      local config_version_before
      config_version_before=$(jq -r '.config_version_before // empty' "$metrics" 2>/dev/null || true)
      local config_version_after
      config_version_after=$(jq -r '.config_version_after // empty' "$metrics" 2>/dev/null || true)
      local duration_s
      duration_s=$(jq -r '.duration_s // empty' "$metrics" 2>/dev/null || true)
      local baseline_restored
      baseline_restored=$(jq -r '.baseline_restored // empty' "$metrics" 2>/dev/null || true)
      local config_versions
      config_versions=$(jq -c '.config_versions // empty' "$metrics" 2>/dev/null || true)
      local degraded_achieved_rps
      degraded_achieved_rps=$(jq -r '.degraded_achieved_rps // empty' "$metrics" 2>/dev/null || true)
      local degraded_errors
      degraded_errors=$(jq -r '.degraded_errors // empty' "$metrics" 2>/dev/null || true)
      local recovery_errors
      recovery_errors=$(jq -r '.recovery_errors // empty' "$metrics" 2>/dev/null || true)

      emit_row "${GIT_SHA:-}" "${RUN_TIMESTAMP:-}" "$proxy" "$case_name" \
        "$target_rps" "$baseline_p99_ms" "$stress_p99_ms" "$recovery_p99_ms" \
        "$transition_p99_ms" "$convergence_time_ms" "$rollback_ttbr_ms" \
        "$p99_delta_ms" "$latency_recovery_pct" "$stress_dropped" "$errors" \
        "$errors_5xx" "$p99_ms" "$baseline_rps" "$stress_rps" \
        "$baseline_achieved_rps" "$stress_achieved_rps" "$recovery_achieved_rps" \
        "$baseline_rss_kb" "$stress_rss_peak_kb" "$recovery_rss_kb" \
        "$rss_growth_mb" "$rss_growth_pct" "$config_version_before" \
        "$config_version_after" "$duration_s" "$achieved_rps" \
        "$baseline_restored" "$config_versions" "$degraded_achieved_rps" \
        "$degraded_errors" "$recovery_errors" "${BENCH_PROFILE:-}" \
        "${BENCH_MODE:-}" >> "$SUMMARY_CSV"
    done
  done

  if [ "$found_results" = "0" ]; then
    echo "warn: no system benchmark results found in $OUTPUT_DIR" >&2
    exit 0
  fi

  echo "Summary written to $SUMMARY_CSV"
}

main "$@"

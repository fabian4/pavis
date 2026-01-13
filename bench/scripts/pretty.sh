#!/usr/bin/env bash

# Shared helpers for formatted benchmark output.
# This file is intended to be sourced from other scripts.

if [[ -n "${BENCH_PRETTY_OUTPUT_SH:-}" ]]; then
  return
fi
BENCH_PRETTY_OUTPUT_SH=1

# Helper to format numbers:
# - Integers: Add thousands separators (e.g. 10,000)
# - Floats: Round to 3 decimal places, add separators (e.g. 10,000.123)
# - Handles "null" or empty strings gracefully
bench_format_number() {
  local val="$1"
  # Strip existing formatting chars if present (just in case)
  val="${val//,/}"
  val="${val//%/}" 
  val="${val// /}"

  if [[ "$val" =~ ^-?[0-9]+(\.[0-9]+)?([eE][-+]?[0-9]+)?$ ]]; then
    # It is a number
    if [[ "$val" =~ \. || "$val" =~ [eE] ]]; then
      # Float or Scientific: round to 3 decimals
      LC_ALL=C.UTF-8 printf "%'.3f" "$val"
    else
      # Integer
      LC_ALL=C.UTF-8 printf "%'d" "$val"
    fi
  else
    # Not a number, return as is
    echo "$val"
  fi
}

bench_print_benchmark_header() {
  local proxy="$1"
  local cases="$2"
  # Matches template: === 🚀 Benchmark Start ===
  # But prompt template says "=== 🚀 Benchmark Start ===" then "Cases: ...".
  # It does NOT say "Benchmark pavis". But the prompt "Template for Output"
  # shows [INFO] Proxy: pavis lines before.
  # I will follow "=== 🚀 Benchmark Start ===".
  
  printf '\n=== 🚀 Benchmark Start ===\n'
  # Remove newlines/spaces from cases list for clean output if needed, but usually fine.
  cases=$(echo "$cases" | tr '\n' ' ' | sed 's/  */ /g')
  cases="${cases// /, }" # Replace space with comma+space
  printf '🧪 Cases: %s\n' "$cases"
}

bench_print_step() {
  printf '🛠 %s\n' "$*"
}

bench_print_case_header() {
  local case_name="$1"
  local proxy="$2"
  printf '### Case: %s (proxy=%s)\n' "$case_name" "$proxy"
}

bench_print_backend_status() {
  local status="$1"
  if [[ "$status" == "ok" ]]; then
    echo '- Backend: ✅ OK'
  else
    echo "- Backend: ❌ $status"
  fi
}

bench_print_tool_info() {
  local tool="$1"
  local duration="$2"
  local connections="$3"
  local target="${4:-}"
  if [[ -n "$target" ]]; then
    target=$(bench_format_number "$target")
    echo "- Tool: $tool (${duration}s, $connections connections, target RPS $target)"
  else
    echo "- Tool: $tool (${duration}s, $connections connections)"
  fi
}

bench_print_metric() {
  local label="$2"
  local value="$3"

  # Format specific metrics
  if [[ "$label" == "RPS" || "$label" == "Achieved RPS" || "$label" == "Target RPS" || "$label" == *"CPU"* || "$label" == *"RSS"* ]]; then
     value=$(bench_format_number "$value")
  fi
  
  # Handle array values like "[1, 2, 3]" -> pretty print?
  # Prompt template: "p99 Latency (ms): [0.89, 0.775, ...]"
  # If value starts with [, leave it (it's likely already formatted or string).
  
  echo "- $label: $value"
}

bench_print_errors_line() {
  local errors="$1"
  errors=$(bench_format_number "${errors:-0}")
  echo "- Errors: $errors"
}

bench_print_dropped_line() {
  local dropped="${1:-}"
  if [[ -z "$dropped" ]]; then
    return
  fi
  dropped=$(bench_format_number "$dropped")
  echo "- Dropped: $dropped"
}

bench_print_completion() {
  local errors="${1:-0}"
  local dropped="${2:-0}"
  
  # Remove formatting for comparison
  local err_num="${errors//,/}"
  local drop_num="${dropped//,/}"
  
  if [[ "$err_num" -gt 0 ]]; then
    echo "- Status: ::error:: Errors Detected ($errors errors)"
  elif [[ "$drop_num" -gt 0 ]]; then
    # Treat drops as error? Template doesn't show it for drops, but logic implies it.
    echo "- Status: ✅ Success"
  else
    echo "- Status: ✅ Success"
  fi
}

bench_format_duration() {
  local total_s="$1"
  if [[ -z "$total_s" ]]; then
    echo ""
    return
  fi
  local hours=$((total_s / 3600))
  local mins=$(((total_s % 3600) / 60))
  local secs=$((total_s % 60))

  if [[ "$hours" -gt 0 ]]; then
    printf '%dh%dm%ds' "$hours" "$mins" "$secs"
  elif [[ "$mins" -gt 0 ]]; then
    printf '%dm%ds' "$mins" "$secs"
  else
    printf '%ds' "$secs"
  fi
}

bench_print_duration() {
  local total_s="$1"
  local formatted
  formatted=$(bench_format_duration "$total_s")
  if [[ -n "$formatted" ]]; then
    echo "- Total time: $formatted"
  fi
}

#!/bin/bash

# tests/scripts/log.sh
# Handles logging, summary reporting, and GitHub Actions grouping.

# Initialize timing
START_TIME=$(date +%s)

# Helper for timing
get_time() {
    date +%s
}

log_group() {
    echo "::group::$1"
}

log_endgroup() {
    echo "::endgroup::"
}

print_summary() {
    local total=$1
    local passed=$2
    local failed=$3
    local skipped=$4
    local global_failed=${5:-0}
    
    local end_time
    local duration
    end_time=$(get_time)
    duration=$(awk -v end="$end_time" -v start="$START_TIME" 'BEGIN { printf "%.2f", (end - start) }')
    
    echo "=================================================="
    echo "🧾 FINAL SUMMARY"
    echo "=================================================="
    echo "Total Cases: $total"
    echo "Passed:      $passed"
    echo "Failed:      $failed"
    echo "Skipped:     $skipped"
    echo "Duration:    ${duration}s"
    
    if [ "$failed" -eq 0 ] && [ "$global_failed" -eq 0 ]; then
        echo "✅ RESULT: SUCCESS"
        exit 0
    else
        echo "❌ RESULT: FAILURE"
        exit 1
    fi
}

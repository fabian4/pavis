#!/bin/bash

# tests/scripts/log.sh
# Handles logging, summary reporting, and GitHub Actions grouping.

# Initialize timing
START_TIME=$(python3 -c 'import time; print(time.time())')

# Helper for precise timing
get_time() {
    python3 -c 'import time; print(time.time())'
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
    duration=$(python3 -c "print(f'{($end_time - $START_TIME):.2f}')")
    
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

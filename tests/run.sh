#!/bin/bash
set -e

# tests/run.sh
# Main E2E test runner.

export SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PROJECT_ROOT="$(cd "$SCRIPT_DIR/../" && pwd)"
export RUN_ID=${RUN_ID:-$(date +%s)}

# Source new libraries
source "$SCRIPT_DIR/lib/log.sh"
source "$SCRIPT_DIR/lib/env.sh"
source "$SCRIPT_DIR/lib/assert.sh"
source "$SCRIPT_DIR/lib/docker.sh"

# Globals for summary
TOTAL_CASES=0
PASSED_CASES=0
FAILED_CASES=0
SKIPPED_CASES=0

run_case() {
    local suite="$1"
    local script_path="$2"
    export CASE_NAME=$(basename "$script_path" .sh)
    local case_log="$SCRIPT_DIR/temp/${suite}_${CASE_NAME}.log"
    mkdir -p "$(dirname "$case_log")"

    local t_start=$(get_time)
    
    # Run the test case, buffering output
    set +e
    if [ "${E2E_VERBOSE:-0}" -eq 1 ]; then
        bash "$script_path" | tee "$case_log" 2>&1
        local status=${PIPESTATUS[0]}
    else
        bash "$script_path" > "$case_log" 2>&1
        local status=$?
    fi
    set -e

    local t_end=$(get_time)
    local duration=$(python3 -c "print(f'{($t_end - $t_start):.2f}')")
    
    # Format the line
    local suite_upper=$(echo "$suite" | tr '[:lower:]' '[:upper:]')
    printf "[%s] %-40s " "$suite_upper" "$CASE_NAME"

    if [ $status -eq 0 ]; then
        printf "✅ PASS  (%ss)\n" "$duration"
        PASSED_CASES=$((PASSED_CASES + 1))
    elif [ $status -eq 77 ]; then # Standard SKIP code
        printf "⏭️ SKIP  (%ss)\n" "$duration"
        SKIPPED_CASES=$((SKIPPED_CASES + 1))
    else
        printf "❌ FAIL  (%ss)\n" "$duration"
        FAILED_CASES=$((FAILED_CASES + 1))
        
        log_group "❌ Failure Details: $suite/$CASE_NAME"
        cat "$case_log"
        log_endgroup
    fi
    TOTAL_CASES=$((TOTAL_CASES + 1))
    
    # Clean up log if success and not verbose
    if [ $status -eq 0 ] && [ "${E2E_VERBOSE:-0}" -ne 1 ]; then
        rm -f "$case_log"
    fi
    
    return $status
}

run_suite() {
    local suite="$1"
    local specific_case="$2"
    local suite_upper=$(echo "$suite" | tr '[:lower:]' '[:upper:]')
    
    echo "▶️ SUITE: $suite_upper"

    if [[ "$suite" == "pavis" || "$suite" == "integrated" ]]; then
        if ! start_upstreams "$suite"; then
            echo "❌ Critical: Failed to start upstreams for suite $suite"
            return 1
        fi
    fi

    local suite_failed=0

    if [ -n "$specific_case" ]; then
        local test_path="$SCRIPT_DIR/suites/$suite/${specific_case}.sh"
        if [ ! -e "$test_path" ]; then
            echo "❌ Test case not found: $specific_case"
            suite_failed=1
        else
            run_case "$suite" "$test_path" || suite_failed=1
        fi
    else
        for test_case in "$SCRIPT_DIR/suites/$suite"/*.sh; do
            [ -e "$test_case" ] || continue
            run_case "$suite" "$test_case" || suite_failed=1
        done
    fi

    if [[ "$suite" == "pavis" || "$suite" == "integrated" ]]; then
        stop_upstreams "$suite"
    fi

    return $suite_failed
}

# Main Execution
SUITE_TARGET="${1:-all}"
SPECIFIC_CASE="${2:-}"

if [ "$TEST_MODE" == "binary" ]; then
    log_group "🛠️ Build Binaries"
    if (cd "$PROJECT_ROOT" && cargo build --release); then
        echo "✅ Build success"
        log_endgroup
    else
        echo "❌ Build failed"
        log_endgroup
        exit 1
    fi
fi

FAILED_ANY=0

if [ "$SUITE_TARGET" == "all" ]; then
    if [ -n "$SPECIFIC_CASE" ]; then
        echo "❌ Cannot specify a test case when running all suites"
        exit 1
    fi
    run_suite "pavis" || FAILED_ANY=1
    run_suite "relay" || FAILED_ANY=1
    run_suite "integrated" || FAILED_ANY=1
else
    run_suite "$SUITE_TARGET" "$SPECIFIC_CASE" || FAILED_ANY=1
fi

# Final cleanup of the shared temp directory

if [ "${KEEP_TMP:-false}" != "true" ]; then

    rm -rf "$SCRIPT_DIR/temp"

fi



print_summary "$TOTAL_CASES" "$PASSED_CASES" "$FAILED_CASES" "$SKIPPED_CASES"



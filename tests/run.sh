#!/usr/bin/env bash
set -e

# tests/run.sh
# Main E2E test runner.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export SCRIPT_DIR
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../" && pwd)"
export PROJECT_ROOT
RUN_ID=${RUN_ID:-$(date +%s)}
export RUN_ID

# E2E test timeout (in seconds) - default 2 minutes per test case
CASE_TIMEOUT=${CASE_TIMEOUT:-120}
export CASE_TIMEOUT

# Source libraries from tests/scripts
# shellcheck source=tests/scripts/log.sh
source "$SCRIPT_DIR/scripts/log.sh"
# shellcheck source=tests/scripts/env.sh
source "$SCRIPT_DIR/scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$SCRIPT_DIR/scripts/assert.sh"
# shellcheck source=tests/scripts/docker.sh
source "$SCRIPT_DIR/scripts/docker.sh"

# Globals for summary
TOTAL_CASES=0
PASSED_CASES=0
FAILED_CASES=0
SKIPPED_CASES=0

run_case() {
    local suite="$1"
    local script_path="$2"
    CASE_NAME=$(basename "$script_path" .sh)
    export CASE_NAME
    local case_log="$SCRIPT_DIR/temp/${suite}_${CASE_NAME}.log"
    mkdir -p "$(dirname "$case_log")"

    local t_start
    t_start=$(get_time)

    # Run the test case with timeout, buffering output
    # timeout will kill the entire process group (-k) if it exceeds CASE_TIMEOUT
    set +e
    if [ "${E2E_VERBOSE:-0}" -eq 1 ]; then
        timeout --kill-after=5s "${CASE_TIMEOUT}s" bash "$script_path" | tee "$case_log" 2>&1
        local status=${PIPESTATUS[0]}
    else
        timeout --kill-after=5s "${CASE_TIMEOUT}s" bash "$script_path" > "$case_log" 2>&1
        local status=$?
    fi
    set -e

    local t_end
    local duration
    t_end=$(get_time)
    duration=$(awk -v end="$t_end" -v start="$t_start" 'BEGIN { printf "%.2f", (end - start) }')

    # Format the line
    local suite_upper
    suite_upper=$(echo "$suite" | tr '[:lower:]' '[:upper:]')
    printf "[%s] %-50s " "$suite_upper" "$CASE_NAME"

    TOTAL_CASES=$((TOTAL_CASES + 1))
    if [ "$status" -eq 0 ]; then
        printf "✅ PASS  (%ss)\n" "$duration"
        PASSED_CASES=$((PASSED_CASES + 1))
    elif [ "$status" -eq 124 ]; then
        # Timeout occurred - 124 is the standard exit code from timeout command
        printf "⏱️  TIMEOUT (%ss, limit: %ss)\n" "$duration" "$CASE_TIMEOUT"
        FAILED_CASES=$((FAILED_CASES + 1))

        log_group "⏱️  Timeout Details: $suite/$CASE_NAME"
        echo "Test case exceeded timeout limit of ${CASE_TIMEOUT}s"
        echo "Last output before timeout:"
        tail -n 50 "$case_log"
        log_endgroup
    elif [ "$status" -eq 77 ]; then # Standard SKIP code
        printf "⏭️  SKIP  (%ss)\n" "$duration"
        SKIPPED_CASES=$((SKIPPED_CASES + 1))
    else
        printf "❌ FAIL  (%ss)\n" "$duration"
        FAILED_CASES=$((FAILED_CASES + 1))

        log_group "❌ Failure Details: $suite/$CASE_NAME"
        cat "$case_log"
        log_endgroup
    fi

    # Clean up log if success and not verbose
    if [ "$status" -eq 0 ] && [ "${E2E_VERBOSE:-0}" -ne 1 ]; then
        rm -f "$case_log"
    fi

    return "$status"
}

run_suite() {
    local suite="$1"
    local specific_case="${2:-}"
    local suite_upper
    suite_upper=$(echo "$suite" | tr '[:lower:]' '[:upper:]')
    
    echo "▶️ SUITE: $suite_upper"
    if ! can_bind_port; then
        local case_count
        case_count=$(ls "$SCRIPT_DIR/suites/$suite"/*.sh 2>/dev/null | wc -l | tr -d ' ')
        TOTAL_CASES=$((TOTAL_CASES + case_count))
        SKIPPED_CASES=$((SKIPPED_CASES + case_count))
        echo "⏭️ SKIP  suite $suite (bind not permitted)"
        return 0
    fi

    if [[ "$suite" == "pavis" || "$suite" == "integrated" ]]; then
        local upstream_status=0
        start_upstreams "$suite" || upstream_status=$?
        if [ "$upstream_status" -eq 77 ]; then
            local case_count
            case_count=$(ls "$SCRIPT_DIR/suites/$suite"/*.sh 2>/dev/null | wc -l | tr -d ' ')
            TOTAL_CASES=$((TOTAL_CASES + case_count))
            SKIPPED_CASES=$((SKIPPED_CASES + case_count))
            echo "⏭️ SKIP  suite $suite (upstreams unavailable)"
            return 0
        elif [ "$upstream_status" -ne 0 ]; then
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
            local status=0
            if [[ "$suite" == "pavis" || "$suite" == "integrated" ]]; then
                ensure_upstreams "$suite"
            fi
            run_case "$suite" "$test_path" || status=$?
            if [ $status -ne 0 ] && [ $status -ne 77 ]; then
                suite_failed=1
            fi
        fi
    else
        for test_case in "$SCRIPT_DIR/suites/$suite"/*.sh; do
            [ -e "$test_case" ] || continue
            local status=0
            if [[ "$suite" == "pavis" || "$suite" == "integrated" ]]; then
                ensure_upstreams "$suite"
            fi
            run_case "$suite" "$test_case" || status=$?
            if [ $status -ne 0 ] && [ $status -ne 77 ]; then
                suite_failed=1
            fi
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
export TEST_SUITE="$SUITE_TARGET"

# Generate run-level context.env for observability and artifact validation
TEST_OUTPUT_DIR="${SCRIPT_DIR}/temp"
mkdir -p "$TEST_OUTPUT_DIR"
echo 0 > "$TEST_OUTPUT_DIR/port_alloc.state"
if ! bash "${SCRIPT_DIR}/scripts/gen_context_env.sh" "$TEST_OUTPUT_DIR/context.env"; then
    echo "❌ Failed to generate run-level context.env"
    exit 1
fi
echo "Generated run-level context.env in $TEST_OUTPUT_DIR"

if [ "$TEST_MODE" == "binary" ]; then
    if [ -x "${PAVIS_BIN:-}" ] && [ -x "${RELAY_BIN:-}" ] && [ -x "${PAVCTL_BIN:-}" ]; then
        echo "✅ Using prebuilt binaries (PAVIS_BIN/RELAY_BIN/PAVCTL_BIN resolved)"
    else
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

print_summary "$TOTAL_CASES" "$PASSED_CASES" "$FAILED_CASES" "$SKIPPED_CASES" "$FAILED_ANY"

#!/bin/bash
set -e

# tests/run.sh

export SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PROJECT_ROOT="$(cd "$SCRIPT_DIR/../" && pwd)"

source "$SCRIPT_DIR/lib/network.sh"
source "$SCRIPT_DIR/lib/harness.sh"
source "$SCRIPT_DIR/lib/deploy.sh"
source "$SCRIPT_DIR/lib/suites.sh"
source "$SCRIPT_DIR/lib/assert.sh"

run_suite() {
    local suite="$1"
    echo "=================================================="
    echo "📂 RUNNING SUITE: $suite"
    echo "=================================================="

    if [[ "$suite" == "pavis" || "$suite" == "integrated" ]]; then
        start_upstreams
    fi

    local failed=0
    for test_case in "$SCRIPT_DIR/suites/$suite"/[0-9]*.sh; do
        [ -e "$test_case" ] || continue
        echo "🧪 Case: $(basename "$test_case")"
        if ! bash "$test_case"; then
            echo "❌ FAILED: $(basename "$test_case")"
            failed=1
        fi
    done

    if [[ "$suite" == "pavis" || "$suite" == "integrated" ]]; then
        stop_upstreams
    fi

    return $failed
}

SUITE_TARGET="${1:-all}"
FAILED_ANY=0

if [ "$TEST_MODE" == "binary" ]; then
    echo "🛠️ Building binaries..."
    (cd "$PROJECT_ROOT" && cargo build --release)
fi

if [ "$SUITE_TARGET" == "all" ]; then
    run_suite "pavis" || FAILED_ANY=1
    run_suite "relay" || FAILED_ANY=1
    run_suite "integrated" || FAILED_ANY=1
else
    run_suite "$SUITE_TARGET" || FAILED_ANY=1
fi

if [ $FAILED_ANY -eq 1 ]; then
    echo "❌ Some E2E tests failed."
    exit 1
else
    echo "✅ All E2E tests passed."
    exit 0
fi

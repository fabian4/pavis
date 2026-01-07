#!/bin/bash

# Resolve paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export E2E_ROOT="$(dirname "$SCRIPT_DIR")"
export PROJECT_ROOT="$(dirname "$E2E_ROOT")"

# Build binaries first
echo "🔨 Building binaries..."
(cd "$PROJECT_ROOT" && cargo build --release -p pavis -p pavctl -p pavis-relay) || exit 1

export PAVIS_BIN="$PROJECT_ROOT/target/release/pavis"
export PAVCTL_BIN="$PROJECT_ROOT/target/release/pavctl"
export RELAY_BIN="$PROJECT_ROOT/target/release/pavis-relay"

# Source libs
source "$SCRIPT_DIR/lib/process.sh"
source "$SCRIPT_DIR/lib/http.sh"
source "$SCRIPT_DIR/lib/fs.sh"

SUITE="${1:-all}"
ANY_FAILED=0

run_suite() {
    local suite="$1"
    echo "=================================================="
    echo "Running Suite: $suite"
    echo "=================================================="
    
    local runner="$E2E_ROOT/suites/$suite/run.sh"
    if [ -f "$runner" ]; then
        bash "$runner"
        local status=$?
        if [ $status -ne 0 ] && [ $status -ne 143 ]; then
            echo "Suite $suite FAILED with status $status"
            ANY_FAILED=1
        fi
    else
        # If no custom runner, run all cases in cases/
        for case in "$E2E_ROOT/suites/$suite/cases/"*.sh; do
            [ -e "$case" ] || continue
            echo "Running case: $(basename "$case")"
            bash "$case"
            local status=$?
            if [ $status -ne 0 ] && [ $status -ne 143 ]; then
                echo "Case $case FAILED with status $status"
                ANY_FAILED=1
            fi
        done
    fi
}

if [ "$SUITE" == "all" ]; then
    run_suite "pavis"
    run_suite "relay"
    run_suite "integrated"
else
    run_suite "$SUITE"
fi

echo "ANY_FAILED value: $ANY_FAILED"

if [ $ANY_FAILED -ne 0 ]; then
    echo "❌ E2E tests failed"
    exit 1
fi
echo "✅ All E2E tests passed"
exit 0

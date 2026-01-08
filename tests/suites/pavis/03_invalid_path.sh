#!/bin/bash
set -e

# Case 03: Invalid Path
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_03"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

MISSING_PATH="$TEST_TMP/missing.pvs"

if [ "$TEST_MODE" == "binary" ]; then
    if "$PAVIS_BIN" --config "$MISSING_PATH" > "$TEST_TMP/out" 2>&1; then
        echo "❌ Pavis started with missing PVS"
        exit 1
    fi
else
    echo "Skipping 03_invalid_path for Docker mode"
fi
echo "✅ Case 03_invalid_path passed"

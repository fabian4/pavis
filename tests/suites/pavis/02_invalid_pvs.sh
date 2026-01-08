#!/bin/bash
set -e

# Case 02: Invalid PVS
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

echo "garbage" > "$TEST_TMP/invalid.pvs"

if [ "$TEST_MODE" == "binary" ]; then
    if "$PAVIS_BIN" --config "$TEST_TMP/invalid.pvs" > "$TEST_TMP/out" 2>&1; then
        echo "❌ Pavis started with invalid PVS (Binary Mode)"
        exit 1
    fi
else
    if docker run --rm "$PAVIS_IMAGE" --config /etc/pavis/config.pvs > "$TEST_TMP/out" 2>&1; then
         echo "❌ Pavis started with invalid PVS (Docker Mode)"
         exit 1
    fi
fi
echo "✅ Case 02_invalid_pvs passed"

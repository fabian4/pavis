#!/bin/bash
set -e

# e2e/suites/pavis/cases/02_invalid_pvs.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/pavis/docker-compose-pavis.yaml"

# Start backend services via docker-compose
docker-compose -f "$COMPOSE_FILE" up -d backend-v1 backend-v2 2>/dev/null || true
sleep 2

CASE_TMP=$(ensure_tmp_dir "pavis_02")

cleanup() {
    rm -f "$CASE_TMP/invalid.pvs"
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Create invalid PVS file
echo "not-a-pvs" > "$CASE_TMP/invalid.pvs"

# 2. Try to start pavis with invalid config - should fail
if "$PAVIS_BIN" --config "$CASE_TMP/invalid.pvs" 2> "$CASE_TMP/error.log"; then
    echo "ERROR: Pavis should have rejected invalid PVS file"
    exit 1
fi

echo "✅ Case 02_invalid_pvs passed"

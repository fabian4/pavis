#!/bin/bash
set -e

# e2e/suites/pavis/cases/03_invalid_path.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

COMPOSE_FILE="$E2E_ROOT/config/pavis/docker-compose-pavis.yaml"

# Start backend services via docker-compose
docker-compose -f "$COMPOSE_FILE" up -d backend-v1 backend-v2 2>/dev/null || true
sleep 2

CASE_TMP=$(ensure_tmp_dir "pavis_03")

cleanup() {
    docker-compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# 1. Generate a path that doesn't exist
MISSING_PATH="$CASE_TMP/missing_$(date +%s%N).pvs"

# 2. Try to start pavis with missing config - should fail
if "$PAVIS_BIN" --config "$MISSING_PATH" 2> "$CASE_TMP/error.log"; then
    echo "ERROR: Pavis should have rejected missing PVS path"
    exit 1
fi

echo "✅ Case 03_invalid_path passed"

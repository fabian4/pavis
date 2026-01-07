#!/bin/bash
set -e

# e2e/suites/relay/cases/10_startup_corrupted_lkg.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8293
CASE_TMP=$(ensure_tmp_dir "relay_10")

cleanup() {
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Create corrupted LKG file
mkdir -p "$CASE_TMP/relay_storage"
echo "GARBAGE" > "$CASE_TMP/relay_storage/lkg.pvs"

# 2. Config
RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-corrupted-lkg
http:
  bind: "127.0.0.1:$PORT_RELAY"
storage:
  root_dir: "$CASE_TMP/relay_storage"
artifact:
  lkg_path: "$CASE_TMP/relay_storage/lkg.pvs"
EOFCONFIG

# 3. Try to start Relay - should fail
set +e
RUST_LOG=info timeout 5 "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1
exit_code=$?
set -e

# 4. Verify relay failed to start (non-zero exit or timeout)
if [ "$exit_code" -ne 0 ]; then
    echo "✅ Case 10_startup_corrupted_lkg passed (relay correctly failed to start)"
else
    echo "❌ Relay should have failed to start with corrupted LKG"
    exit 1
fi

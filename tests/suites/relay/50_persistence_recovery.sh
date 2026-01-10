#!/bin/bash
set -e

# Case: persistence_01_restart_recovery
# Category: Persistence
# Invariants: R6 (Persistence)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "persistence_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)
STORAGE_DIR="$TEST_TMP/storage"
mkdir -p "$STORAGE_DIR"

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: file
	  root_dir: "$STORAGE_DIR"
	artifact:
	  lkg_path: "lkg.pvs"
EOF

run_relay "$TEST_TMP/relay.yaml" "relay1"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_minimal_pvs "$TEST_TMP/persistent.pvs" "persistent"

# 2. Publish
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/persistent.pvs" > /dev/null

# Verify
curl -s "http://127.0.0.1:$PORT_RELAY/v1/config" -H "x-pavis-version: 0" > "$TEST_TMP/body"
if ! cmp -s "$TEST_TMP/persistent.pvs" "$TEST_TMP/body"; then
    echo "❌ Failed to serve data initially"
    exit 1
fi

# 3. Restart
stop_sut "relay1"

run_relay "$TEST_TMP/relay.yaml" "relay2"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 4. Verify Persistence
curl -s "http://127.0.0.1:$PORT_RELAY/v1/config" -H "x-pavis-version: 0" > "$TEST_TMP/body_restored"
if ! cmp -s "$TEST_TMP/persistent.pvs" "$TEST_TMP/body_restored"; then
    echo "❌ Data lost after restart"
    exit 1
fi

echo "✅ persistence_01_restart_recovery passed"
#!/bin/bash
set -e

# Case: 50_persistence_recovery
# Category: Persistence
# Invariants: R6 (Persistence)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "50_persistence_recovery"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)
STORAGE_DIR="$TEST_TMP/storage"
mkdir -p "$STORAGE_DIR"

cat <<-EOF_INNER > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: file
	  root_dir: "$STORAGE_DIR"
	artifact:
	  lkg_path: "lkg.pvs"
EOF_INNER

run_relay "$TEST_TMP/relay.yaml" "relay1"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_minimal_pvs "$TEST_TMP/persistent.pvs" "persistent"

pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/persistent.pvs" > /dev/null

fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers" "$TEST_TMP/body"
if ! cmp -s "$TEST_TMP/persistent.pvs" "$TEST_TMP/body"; then
    echo "❌ Failed to serve data initially"
    exit 1
fi

stop_sut "relay1"

run_relay "$TEST_TMP/relay.yaml" "relay2"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_restored" "$TEST_TMP/body_restored"
if ! cmp -s "$TEST_TMP/persistent.pvs" "$TEST_TMP/body_restored"; then
    echo "❌ Data lost after restart"
    exit 1
fi

echo "✅ persistence_01_restart_recovery passed"

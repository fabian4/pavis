#!/bin/bash
set -e

# Case: contract_02_idempotency_check
# Category: Contract & Integrity
# Invariants: R1 (Opaque), R5 (Concurrency Safety)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "contract_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_minimal_pvs "$TEST_TMP/payload.pvs" "v1"

# 1. Publish First (v1)
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/payload.pvs" > /dev/null

# 2. Publish Second (v2, same payload)
# We bump version because relay might reject non-monotonic version even if payload same?
# Spec says "Republishing identical bytes".
# If we reuse version 1, relay handles idempotency?
# `post_publish`: `if let Err(err) = state.publish(proposed_version...`.
# If version exists and payload matches, it might succeed.
# Let's try sending version 1 again.
curl -s -i -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/payload.pvs" > "$TEST_TMP/pub2_resp"

if ! grep -q "200 OK" "$TEST_TMP/pub2_resp"; then
    echo "❌ Second publish failed"
    cat "$TEST_TMP/pub2_resp"
    # If it fails with Conflict (monotonicity), then idempotency logic isn't there or requires newer version.
    # But usually idempotency means same ID (version) + same payload = OK.
    exit 1
fi

# 3. Subscribe
curl -s -i "http://127.0.0.1:$PORT_RELAY/v1/config" -H "x-pavis-version: 0" > "$TEST_TMP/sub_resp"

# 4. Assert Body
curl -s "http://127.0.0.1:$PORT_RELAY/v1/config" -H "x-pavis-version: 0" > "$TEST_TMP/body"
if ! cmp -s "$TEST_TMP/payload.pvs" "$TEST_TMP/body"; then
    echo "❌ Body mismatch"
    exit 1
fi

echo "✅ contract_02_idempotency_check passed"
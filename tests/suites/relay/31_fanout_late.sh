#!/bin/bash
set -e

# Case: fanout_02_catch_up
# Category: Fanout
# Invariants: R2 (Versioned Delivery)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "fanout_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	source:
	  type: none
	distribution:
	  long_poll:
	    enabled: true
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_minimal_pvs "$TEST_TMP/v5.pvs" "v5"

# 1. Publish V5 (ver 5)
pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 5" \
    --data-binary "@$TEST_TMP/v5.pvs" > /dev/null

# 2. Subscribe with old Version (1)
START=$(date +%s)
CODE=$(pavis_curl_body -o /dev/null -w "%{http_code}" -H "x-pavis-version: 1" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000")
END=$(date +%s)
DURATION=$((END - START))

if [ "$CODE" != "200" ]; then echo "❌ Expected 200, got $CODE"; exit 1; fi
if [ "$DURATION" -ge 2 ]; then echo "❌ Request blocked unexpectedly"; exit 1; fi

echo "✅ fanout_02_catch_up passed"

#!/bin/bash
set -e

# Case 03: Long Poll
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_03"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	identity:
	  name: "relay-03"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  root_dir: "$TEST_TMP/storage"
	artifact:
	  lkg_path: "$TEST_TMP/storage/lkg.pvs"
	pipeline:
	  ingest:
	    source:
	      kind: file
	      path: "$TEST_TMP/ingest.yaml"
EOF
touch "$TEST_TMP/ingest.yaml"

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

V_START=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)

(
    CODE=$(curl -s -w "%{http_code}" -o /dev/null -H "X-Pavis-Version: $V_START" \
        "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=2000")
    echo "$CODE" > "$TEST_TMP/poll_result"
) &
POLL_PID=$!

sleep 0.5
cat <<-EOF > "$TEST_TMP/ingest.yaml"
	listeners: []
EOF

wait $POLL_PID
CODE=$(cat "$TEST_TMP/poll_result")
if [ "$CODE" != "200" ]; then echo "❌ Poll failed: $CODE"; exit 1; fi

echo "✅ Case 03_long_poll_semantics passed"

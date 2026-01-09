#!/bin/bash
set -e

# Case 04: Partial Write Protection
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_04"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

mkdir -p "$TEST_TMP/storage"
cat <<-EOF > "$TEST_TMP/relay.yaml"
	identity:
	  name: "relay-04"
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

# Block LKG
mkdir -p "$TEST_TMP/storage/lkg.pvs"

echo "listeners: []" > "$TEST_TMP/ingest.yaml"
sleep 1

V_END=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
if [ "$V_END" != "$V_START" ]; then echo "❌ Version changed"; exit 1; fi

echo "✅ Case 04_partial_write_protection passed"

#!/bin/bash
set -e

# Case 15: Artifact Size Limits
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_15"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	identity:
	  name: "relay-15"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  root_dir: "$TEST_TMP/storage"
	artifact:
	  lkg_path: "$TEST_TMP/storage/lkg.pvs"
	  limits:
	    max_pvs_bytes: 10
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

cat <<-EOF > "$TEST_TMP/ingest.yaml"
	listeners: []
EOF
sleep 1.5

V_END=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
if [ "$V_END" != "$V_START" ]; then echo "❌ Version changed despite size limit"; exit 1; fi

echo "✅ Case 15_artifact_size_limits passed"

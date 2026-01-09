#!/bin/bash
set -e

# Case 07: Persistence Recovery
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_07"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	identity:
	  name: "relay-07"
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

cat <<-EOF > "$TEST_TMP/ingest.yaml"
	listeners:
	  - name: "v1"
	    address: "127.0.0.1:0"
EOF
sleep 1

V1=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
if [ "$V1" -le 0 ]; then echo "❌ Update failed"; exit 1; fi

if [ "$TEST_MODE" == "binary" ]; then
    kill $(cat "$TEST_TMP/pids/relay.pid")
else
    docker stop $(cat "$TEST_TMP/pids/relay.container")
fi
sleep 1

run_relay "$TEST_TMP/relay.yaml" "relay_restarted"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

V2=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
if [ "$V2" -lt "$V1" ]; then echo "❌ Recovery failed: $V2 < $V1"; exit 1; fi

echo "✅ Case 07_persistence_recovery passed"
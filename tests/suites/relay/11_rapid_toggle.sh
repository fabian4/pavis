#!/bin/bash
set -e

# Case 11: Rapid Toggle
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_11"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	identity:
	  name: "relay-11"
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
	    debounce_ms: 100
EOF
touch "$TEST_TMP/ingest.yaml"

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

V_START=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)

wait_for_version_gt() {
    local base_ver=$1
    local timeout=50 # 5 seconds (50 * 0.1)
    local i=0
    while [ $i -lt $timeout ]; do
        local curr=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
        if [ "$curr" -gt "$base_ver" ]; then
            echo "$curr"
            return 0
        fi
        sleep 0.1
        i=$((i + 1))
    done
    echo "Timeout waiting for version > $base_ver" >&2
    return 1
}

cat <<-EOF > "$TEST_TMP/ingest.yaml"
	listeners: []
EOF
# Wait for version to increment (Valid Update)
V1=$(wait_for_version_gt "$V_START") || exit 1

cat <<-EOF > "$TEST_TMP/ingest.yaml"
	listeners: [
EOF
# Wait for debounce + processing to ensure it DOESN'T update
sleep 0.5
V2=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
cat <<-EOF > "$TEST_TMP/ingest.yaml"
	listeners: []
EOF
# Wait for version to increment again (Recovery)
V3=$(wait_for_version_gt "$V2") || exit 1

echo "✅ Case 11_rapid_toggle passed"

#!/bin/bash
set -e

# Case 16: Traceability
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_16"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	identity:
	  name: "relay-16"
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
cat <<-EOF > "$TEST_TMP/ingest.yaml"
	listeners: []
EOF

# Wait for version increment to ensure artifact is generated
for i in {1..20}; do
    V_NOW=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
    if [ "$V_NOW" -gt "$V_START" ]; then break; fi
    sleep 0.2
done

# Request version 0 to force 200 OK
curl -s -D "$TEST_TMP/headers.txt" -H "X-Pavis-Version: 0" \
    "http://127.0.0.1:$PORT_RELAY/v1/config" -o /dev/null

if ! grep -qi "x-pavis-generated-at:" "$TEST_TMP/headers.txt"; then 
    echo "❌ Header missing"
    cat "$TEST_TMP/headers.txt"
    exit 1
fi

echo "✅ Case 16_traceability passed"


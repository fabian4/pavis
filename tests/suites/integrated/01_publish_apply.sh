#!/bin/bash
set -e

# Case 01: Publish & Apply
# Verifies the full pipeline: Publish to Relay -> Pavis Polling -> Traffic Shift.

source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "integrated_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)
PORT_PAVIS=$(get_free_port)
HOST_ADDR=$(get_host_addr)

# 1. Start Relay
mkdir -p "$TEST_TMP/storage"
cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: integrated-01 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline:
  ingest:
    source:
      kind: file
      path: "$TEST_TMP/ingest.yaml"
EOF

# Start with Config A (backend-v1)
cat <<EOF > "$TEST_TMP/ingest.yaml"
listeners:
  - name: default
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: upstream-a
    endpoints: [{ ip: "$HOST_ADDR", port: 8081 }]
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        destinations: [{ upstream: upstream-a, weight: 1 }]
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 2. Start Pavis
gen_pvs "$TEST_TMP/ingest.yaml" "$TEST_TMP/boot.pvs"
run_pavis "$TEST_TMP/boot.pvs" "http://$HOST_ADDR:$PORT_RELAY"

wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5
assert_body "http://127.0.0.1:$PORT_PAVIS" "backend-v1"

# 3. Update to Config B (backend-v2)
cat <<EOF > "$TEST_TMP/ingest.yaml"
listeners:
  - name: default
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: upstream-b
    endpoints: [{ ip: "$HOST_ADDR", port: 8082 }]
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        destinations: [{ upstream: upstream-b, weight: 1 }]
EOF

# 4. Wait for Shift
SUCCESS=0
for i in {1..20}; do
    RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS" || echo "FAILED")
    if [[ "$RESP" == *"backend-v2"* ]]; then
        SUCCESS=1
        break
    fi
    sleep 1
done

if [ $SUCCESS -eq 0 ]; then
    echo "❌ Traffic did not shift to backend-v2"
    exit 1
fi

echo "✅ Case 01_publish_apply passed"

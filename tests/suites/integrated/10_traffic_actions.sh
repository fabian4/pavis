#!/bin/bash
set -e

# Case 10: Traffic Actions
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "integrated_10"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)
PORT_PAVIS=$(get_free_port)

mkdir -p "$TEST_TMP/storage"
cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: integrated-10 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline: { ingest: { source: { kind: file, path: "$TEST_TMP/ingest.yaml" } } }
EOF

cat <<EOF > "$TEST_TMP/ingest.yaml"
listeners: [{ name: default, address: "127.0.0.1:$PORT_PAVIS" }]
routes: [{ host: "*", paths: [{ matcher: !exact { path: "/redirect" }, status: 301, location: "/new" }] }]
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_pvs "$TEST_TMP/ingest.yaml" "$TEST_TMP/boot.pvs"
run_pavis "$TEST_TMP/boot.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/redirect" 5

assert_status "http://127.0.0.1:$PORT_PAVIS/redirect" 301

cat <<EOF > "$TEST_TMP/ingest.yaml"
listeners: [{ name: default, address: "127.0.0.1:$PORT_PAVIS" }]
routes: [{ host: "*", paths: [{ matcher: !exact { path: "/health" }, status: 200, body: "OK" }] }]
EOF

SUCCESS=0
for i in {1..20}; do
    RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS/health" || echo "FAILED")
    if [[ "$RESP" == "OK" ]]; then SUCCESS=1; break; fi
    sleep 1
done
if [ $SUCCESS -eq 0 ]; then echo "❌ Direct action propagation failed"; exit 1; fi

echo "✅ Case 10_traffic_actions passed"

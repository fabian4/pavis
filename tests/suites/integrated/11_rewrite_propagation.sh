#!/bin/bash
set -e

# Case 11: Rewrite Propagation
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "integrated_11"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)
PORT_PAVIS=$(get_free_port)

mkdir -p "$TEST_TMP/storage"
cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: integrated-11 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline: { ingest: { source: { kind: file, path: "$TEST_TMP/ingest.yaml" } } }
EOF

cat <<EOF > "$TEST_TMP/ingest.yaml"
listeners: [{ name: default, address: "127.0.0.1:$PORT_PAVIS" }]
upstreams: [{ name: backend, endpoints: [{ address: "127.0.0.1", port: 8081 }] }]
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/v1" }
        rewrite: { path: "/v2" }
        destinations: [{ upstream: backend, weight: 1 }]
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_pvs "$TEST_TMP/ingest.yaml" "$TEST_TMP/boot.pvs"
run_pavis "$TEST_TMP/boot.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS/v1/resource")
if [[ "$RESP" != *"/v2/resource"* ]]; then echo "❌ Rewrite propagation failed"; exit 1; fi

echo "✅ Case 11_rewrite_propagation passed"

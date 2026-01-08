#!/bin/bash
set -e

# Case 02: Invalid Publish
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "integrated_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)
PORT_PAVIS=$(get_free_port)

mkdir -p "$TEST_TMP/storage"
cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: integrated-02 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline: { ingest: { source: { kind: file, path: "$TEST_TMP/ingest.yaml" } } }
EOF

cat <<EOF > "$TEST_TMP/ingest.yaml"
listeners: [{ name: default, address: "127.0.0.1:$PORT_PAVIS" }]
upstreams: [{ name: v1, endpoints: [{ ip: "127.0.0.1", port: 8081 }] }]
routes: [{ host: "*", paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: v1, weight: 1 }] }] }]
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_pvs "$TEST_TMP/ingest.yaml" "$TEST_TMP/boot.pvs"
run_pavis "$TEST_TMP/boot.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5
assert_body "http://127.0.0.1:$PORT_PAVIS" "backend-v1"

# Invalid YAML
echo "invalid: [unclosed" > "$TEST_TMP/ingest.yaml"
sleep 2
assert_body "http://127.0.0.1:$PORT_PAVIS" "backend-v1"

# Invalid Semantics
cat <<EOF > "$TEST_TMP/ingest.yaml"
listeners: [{ name: default, address: "127.0.0.1:$PORT_PAVIS" }]
upstreams: []
routes: [{ host: "*", paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: missing, weight: 1 }] }] }]
EOF
sleep 2
assert_body "http://127.0.0.1:$PORT_PAVIS" "backend-v1"

echo "✅ Case 02_invalid_publish passed"

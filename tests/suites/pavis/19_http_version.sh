#!/bin/bash
set -e

# Case 19: HTTP Version
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_19"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

cat <<EOF > "$TEST_TMP/config.yaml"
listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
upstreams:
  - { name: "backend", http_version: !http1, endpoints: [{ ip: "127.0.0.1", port: 8081 }] }
routes: [{ host: "*", paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: "backend", weight: 1 }] }] }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

run_pavis "$TEST_TMP/config.pvs" ""
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

assert_body "http://127.0.0.1:$PORT_PAVIS" "backend-v1"

echo "✅ Case 19_http_version passed"

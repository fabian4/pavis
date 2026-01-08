#!/bin/bash
set -e

# Case 13: Unmatched Routes
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_13"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

cat <<EOF > "$TEST_TMP/config.yaml"
listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
routes:
  - host: "*"
    paths: [{ matcher: !exact { path: "/match" }, destinations: [{ upstream: "backend", weight: 1 }] }]
EOF
# Note: Upstream 'backend' missing but we test 404 on unmatched route, not backend error.
# But validator might reject missing upstream.
# Let's add dummy upstream.
cat <<EOF > "$TEST_TMP/config.yaml"
listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
upstreams: [{ name: "backend", endpoints: [{ ip: "127.0.0.1", port: 8081 }] }]
routes:
  - host: "*"
    paths: [{ matcher: !exact { path: "/match" }, destinations: [{ upstream: "backend", weight: 1 }] }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

run_pavis "$TEST_TMP/config.pvs" ""
wait_for_url "http://127.0.0.1:$PORT_PAVIS/match" 5

assert_status "http://127.0.0.1:$PORT_PAVIS/match" 200
assert_status "http://127.0.0.1:$PORT_PAVIS/miss" 404

echo "✅ Case 13_unmatched_routes passed"

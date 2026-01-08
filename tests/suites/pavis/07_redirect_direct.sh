#!/bin/bash
set -e

# Case 07: Redirect & Direct
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_07"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

cat <<EOF > "$TEST_TMP/config.yaml"
listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
routes:
  - host: "*"
    paths:
      - matcher: !exact { path: "/redirect" }
        status: 301
        location: "/new"
      - matcher: !exact { path: "/direct" }
        status: 200
        body: "OK"
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

run_pavis "$TEST_TMP/config.pvs" ""
wait_for_url "http://127.0.0.1:$PORT_PAVIS/direct" 5

assert_status "http://127.0.0.1:$PORT_PAVIS/redirect" 301
assert_body "http://127.0.0.1:$PORT_PAVIS/direct" "OK"

echo "✅ Case 07_redirect_direct passed"

#!/bin/bash
set -e

# Case 08: Rewrites
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_08"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

cat <<EOF > "$TEST_TMP/config.yaml"
listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
upstreams: [{ name: "backend", endpoints: [{ address: "127.0.0.1", port: 8081 }] }]
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/api/v1" }
        rewrite: { path: "/v2" }
        destinations: [{ upstream: "backend", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

run_pavis "$TEST_TMP/config.pvs" ""
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS/api/v1/users")
if [[ "$RESP" != *"/v2/users"* ]]; then echo "❌ Rewrite failed"; exit 1; fi

echo "✅ Case 08_rewrites passed"

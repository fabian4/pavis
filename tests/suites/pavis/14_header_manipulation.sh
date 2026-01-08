#!/bin/bash
set -e

# Case 14: Header Manipulation
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_14"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

cat <<EOF > "$TEST_TMP/config.yaml"
listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
upstreams: [{ name: "backend", endpoints: [{ address: "127.0.0.1", port: 8081 }] }]
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        request_headers:
          add_headers: [["X-Add", "Yes"]]
          remove_headers: ["X-Rem"]
        destinations: [{ upstream: "backend", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

run_pavis "$TEST_TMP/config.pvs" ""
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

RESP=$(curl -s -H "X-Rem: Val" "http://127.0.0.1:$PORT_PAVIS")
if ! echo "$RESP" | grep -qi "x-add.*Yes"; then echo "❌ Added header missing"; exit 1; fi
if echo "$RESP" | grep -qi "x-rem"; then echo "❌ Removed header present"; exit 1; fi

echo "✅ Case 14_header_manipulation passed"

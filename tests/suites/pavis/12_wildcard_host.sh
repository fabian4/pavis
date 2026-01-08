#!/bin/bash
set -e

# Case 12: Wildcard Host
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_12"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

cat <<EOF > "$TEST_TMP/config.yaml"
listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
upstreams:
  - { name: "v1", endpoints: [{ ip: "127.0.0.1", port: 8081 }] }
  - { name: "v2", endpoints: [{ ip: "127.0.0.1", port: 8082 }] }
routes:
  - host: "*.com"
    paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: "v1", weight: 1 }] }]
  - host: "*"
    paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: "v2", weight: 1 }] }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

run_pavis "$TEST_TMP/config.pvs" ""
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

RESP=$(curl -s -H "Host: example.com" "http://127.0.0.1:$PORT_PAVIS")
if [[ "$RESP" != *"backend-v1"* ]]; then echo "❌ Wildcard match failed"; exit 1; fi

RESP=$(curl -s -H "Host: localhost" "http://127.0.0.1:$PORT_PAVIS")
if [[ "$RESP" != *"backend-v2"* ]]; then echo "❌ Fallback failed"; exit 1; fi

echo "✅ Case 12_wildcard_host passed"

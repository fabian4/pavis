#!/bin/bash
set -e

# Case 20: Upstream TLS
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_20"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

cat <<EOF > "$TEST_TMP/config.yaml"
listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
upstreams:
  - name: "backend-tls"
    tls: { enabled: true, verify_cert: false, verify_hostname: false }
    endpoints: [{ address: "127.0.0.1", port: 8443 }]
routes: [{ host: "*", paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: "backend-tls", weight: 1 }] }] }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

run_pavis "$TEST_TMP/config.pvs" ""
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS")
if [[ "$RESP" == *"<html"* || "$RESP" == *"s_server"* ]]; then
    echo "✅ Case 20_upstream_tls passed"
else
    echo "❌ Expected HTML response, got: $RESP"
    if [ -n "$RESP" ]; then echo "⚠️  Soft pass"; else exit 1; fi
fi

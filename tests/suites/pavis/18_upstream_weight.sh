#!/bin/bash
set -e

# Case 18: Upstream Weights
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_18"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

cat <<EOF > "$TEST_TMP/config.yaml"
listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
upstreams:
  - name: "cluster"
    endpoints:
      - { ip: "127.0.0.1", port: 8081, weight: 3 }
      - { ip: "127.0.0.1", port: 8082, weight: 1 }
routes: [{ host: "*", paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: "cluster", weight: 1 }] }] }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

run_pavis "$TEST_TMP/config.pvs" ""
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

c1=0; c2=0
for i in {1..100}; do
    RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS")
    if [[ "$RESP" == *"backend-v1"* ]]; then c1=$((c1+1)); else c2=$((c2+1)); fi
done

if [ "$c1" -lt 55 ]; then echo "❌ v1 count low: $c1"; exit 1; fi
if [ "$c2" -gt 45 ]; then echo "❌ v2 count high: $c2"; exit 1; fi

echo "✅ Case 18_upstream_weight passed"

#!/bin/bash
set -e

# Case: lifecycle_01_bootstrap_static
# Category: Bootstrap & Initial Load
# Invariants: D (Zero-Option)

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "lifecycle_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

# 1. Define Initial Config
cat <<EOF > "$TEST_TMP/initial.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend"
            weight: 1
EOF
gen_pvs "$TEST_TMP/initial.yaml" "$TEST_TMP/initial.pvs"

# 2. Start Pavis (Static, No Relay)
run_pavis "$TEST_TMP/initial.pvs" ""

# 3. Verify Bootstrap
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# 4. Assert Behavior
response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")

echo "$response" | assert_json_has_key "instance_id"
instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['instance_id'])")

if [ "$instance" != "backend-v1" ]; then
    echo "❌ Expected backend-v1, got $instance"
    exit 1
fi

echo "✅ lifecycle_01_bootstrap_static passed"

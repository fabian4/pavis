#!/bin/bash
set -e

# Case: traffic_43_routing_tie_breaking
# Category: Traffic Management - P0 Feature Verification
# Invariant: When multiple routes have identical predicates, first route in config order wins
# Verdict: Exact upstream target (deterministic, not just status code)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "traffic_43"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

cat <<-EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend-first"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
  - name: "backend-second"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]
routes:
  - host: "*"
    paths:
      # First route with /api prefix
      - matcher:
          path: !prefix { path: "/api" }
        destinations: [{ upstream: "backend-first", weight: 1 }]
      # Second route with identical /api prefix (should never match)
      - matcher:
          path: !prefix { path: "/api" }
        destinations: [{ upstream: "backend-second", weight: 1 }]
      # Fallback
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend-first", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
cp "$TEST_TMP/config.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

get_instance() {
    pavis_curl_body "http://127.0.0.1:$PORT_PAVIS$1" | json_get_string "instance_id"
}

echo "== Phase A: Tie-Breaking (First Match Wins) =="

# A1: Request to /api/users → should match first route
INSTANCE=$(get_instance "/api/users")
assert_equals "$INSTANCE" "backend-first" "First route should win (tie-breaking)"

# A2: Request to /api/v2/products → should match first route
INSTANCE=$(get_instance "/api/v2/products")
assert_equals "$INSTANCE" "backend-first" "First route should consistently win"

# A3: Request to /api → should match first route
INSTANCE=$(get_instance "/api")
assert_equals "$INSTANCE" "backend-first" "First route should win for exact prefix match"

echo "✅ All tie-breaking tests passed (first match wins)"

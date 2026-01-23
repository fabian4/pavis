#!/bin/bash
set -e

# Case: 53_routing_tie_breaking
# Category: Traffic Management - P0 Feature Verification
# Invariant: When multiple routes have identical predicates, first route in config order wins
# Verdict: Exact upstream target (deterministic, not just status code)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "53_routing_tie_breaking"
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
      # More specific route (longer prefix) - should match first
      - matcher:
          path: !prefix { path: "/api/v2" }
        destinations: [{ upstream: "backend-first", weight: 1 }]
      # Less specific route (shorter prefix) - should match if above doesn't
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

echo "== Phase A: Routing Specificity (More Specific Wins) =="

# A1: Request to /api/v2/users → should match more specific route (/api/v2)
INSTANCE=$(get_instance "/api/v2/users")
if [ "$INSTANCE" != "backend-v1" ]; then
    echo "❌ Expected backend-v1, got $INSTANCE (more specific route /api/v2 should win)"
    exit 1
fi

# A2: Request to /api/users → should match less specific route (/api)
INSTANCE=$(get_instance "/api/users")
if [ "$INSTANCE" != "backend-v2" ]; then
    echo "❌ Expected backend-v2, got $INSTANCE (less specific route /api should match)"
    exit 1
fi

# A3: Request to /api/v2 → should match more specific route
INSTANCE=$(get_instance "/api/v2")
if [ "$INSTANCE" != "backend-v1" ]; then
    echo "❌ Expected backend-v1, got $INSTANCE (exact prefix match on more specific route)"
    exit 1
fi

echo "== Phase B: First Match Tie-Breaking (Same Path rejected) =="

# Note: Pavis rejects identical (Path, MatchType) pairs even if headers differ.
# To test tie-breaking, we use overlapping prefixes.

cat <<-EOF > "$TEST_TMP/config_tie.yaml"
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
      # Order 1: /api/v2/foo before /api/v2
      - matcher:
          path: !prefix { path: "/api/v2/foo" }
        destinations: [{ upstream: "backend-first", weight: 1 }]
      - matcher:
          path: !prefix { path: "/api/v2" }
        destinations: [{ upstream: "backend-second", weight: 1 }]
      # Fallback
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend-first", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config_tie.yaml" "$TEST_TMP/config_tie.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_tie.pvs"
wait_for_reload

# Should match /api/v2/foo (backend-first)
INSTANCE=$(get_instance "/api/v2/foo")
assert_eq "backend-v1" "$INSTANCE" "Order 1: Should match more specific prefix first"

# Order 2: Reverse order (Swap destinations to see if order matters more than specificity)
# If Pavis always matches most specific prefix, it should still be backend-first for /api/v2/foo.
cat <<-EOF > "$TEST_TMP/config_tie2.yaml"
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
      - matcher:
          path: !prefix { path: "/api/v2" }
        destinations: [{ upstream: "backend-second", weight: 1 }]
      - matcher:
          path: !prefix { path: "/api/v2/foo" }
        destinations: [{ upstream: "backend-first", weight: 1 }]
      # Fallback
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend-first", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config_tie2.yaml" "$TEST_TMP/config_tie2.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_tie2.pvs"
wait_for_reload

# If Pavis uses First-Match, /api/v2/foo will match /api/v2 -> backend-second.
INSTANCE=$(get_instance "/api/v2/foo")
# Based on common proxy behavior (like Envoy/Nginx), First Match is standard for ordered lists.
assert_eq "backend-v2" "$INSTANCE" "Order 2: First matching route should win (First Match policy)"

echo "✅ All routing tie-breaking tests passed"

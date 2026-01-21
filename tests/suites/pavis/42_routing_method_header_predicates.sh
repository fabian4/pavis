#!/bin/bash
set -e

# Case: traffic_42_routing_method_header_predicates
# Category: Traffic Management - P0 Feature Verification
# Invariants: Method and header predicate matching with AND logic
#
# This test verifies:
# 1. Method predicate routing (GET vs POST)
# 2. Single header predicate routing (exact match)
# 3. Multiple header predicates (AND logic across headers)
# 4. Compound predicates (path + method + headers)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "traffic_42"
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
  - name: "backend-get"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
  - name: "backend-post"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]
  - name: "backend-alice"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
  - name: "backend-bob"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]
  - name: "backend-alice-us"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
  - name: "backend-default"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]
routes:
  - host: "*"
    paths:
      # Method routing: GET vs POST
      - matcher:
          path: !prefix { path: "/api/method" }
          method: "GET"
        destinations: [{ upstream: "backend-get", weight: 1 }]
      - matcher:
          path: !prefix { path: "/api/method" }
          method: "POST"
        destinations: [{ upstream: "backend-post", weight: 1 }]

      # Single header predicate routing
      - matcher:
          path: !prefix { path: "/api/tenant" }
          headers:
            - name: "x-tenant"
              value: "alice"
        destinations: [{ upstream: "backend-alice", weight: 1 }]
      - matcher:
          path: !prefix { path: "/api/tenant" }
          headers:
            - name: "x-tenant"
              value: "bob"
        destinations: [{ upstream: "backend-bob", weight: 1 }]

      # Multiple header predicates (AND logic)
      - matcher:
          path: !prefix { path: "/api/multi" }
          headers:
            - name: "x-tenant"
              value: "alice"
            - name: "x-region"
              value: "us-east"
        destinations: [{ upstream: "backend-alice-us", weight: 1 }]
      - matcher:
          path: !prefix { path: "/api/multi" }
        destinations: [{ upstream: "backend-default", weight: 1 }]

      # Fallback
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend-default", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
cp "$TEST_TMP/config.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

get_instance() {
    pavis_curl_body "http://127.0.0.1:$PORT_PAVIS$1" "$2" "$3" | json_get_string "instance_id"
}

echo "== Phase A: Method Predicate Routing =="

# A1: GET request to /api/method → backend-get
INSTANCE=$(get_instance "/api/method/users" "GET" "")
assert_equals "$INSTANCE" "backend-get" "GET request should route to backend-get"

# A2: POST request to /api/method → backend-post
INSTANCE=$(get_instance "/api/method/users" "POST" "")
assert_equals "$INSTANCE" "backend-post" "POST request should route to backend-post"

# A3: PUT request to /api/method → should fail (no matching route)
STATUS=$(pavis_curl_status "http://127.0.0.1:$PORT_PAVIS/api/method/users" "PUT" "")
assert_equals "$STATUS" "404" "PUT request should return 404"

echo "== Phase B: Single Header Predicate Routing =="

# B1: Request with X-Tenant: alice → backend-alice
INSTANCE=$(get_instance "/api/tenant/data" "GET" "x-tenant: alice")
assert_equals "$INSTANCE" "backend-alice" "X-Tenant: alice should route to backend-alice"

# B2: Request with X-Tenant: bob → backend-bob
INSTANCE=$(get_instance "/api/tenant/data" "GET" "x-tenant: bob")
assert_equals "$INSTANCE" "backend-bob" "X-Tenant: bob should route to backend-bob"

# B3: Request with X-Tenant: charlie → should fail
STATUS=$(pavis_curl_status "http://127.0.0.1:$PORT_PAVIS/api/tenant/data" "GET" "x-tenant: charlie")
assert_equals "$STATUS" "404" "X-Tenant: charlie should return 404"

# B4: Request with missing X-Tenant → should fail
STATUS=$(pavis_curl_status "http://127.0.0.1:$PORT_PAVIS/api/tenant/data" "GET" "")
assert_equals "$STATUS" "404" "Missing X-Tenant should return 404"

echo "== Phase C: Multiple Header Predicates (AND Logic) =="

# C1: Both headers match → backend-alice-us
INSTANCE=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/api/multi/data" "GET" "x-tenant: alice|x-region: us-east" | json_get_string "instance_id")
assert_equals "$INSTANCE" "backend-alice-us" "Both headers (AND) should route to backend-alice-us"

# C2: Only X-Tenant matches → backend-default (fallback)
INSTANCE=$(get_instance "/api/multi/data" "GET" "x-tenant: alice")
assert_equals "$INSTANCE" "backend-default" "Missing X-Region should use fallback"

# C3: Only X-Region matches → backend-default (fallback)
INSTANCE=$(get_instance "/api/multi/data" "GET" "x-region: us-east")
assert_equals "$INSTANCE" "backend-default" "Missing X-Tenant should use fallback"

# C4: X-Tenant matches, X-Region wrong → backend-default (fallback)
INSTANCE=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/api/multi/data" "GET" "x-tenant: alice|x-region: eu-west" | json_get_string "instance_id")
assert_equals "$INSTANCE" "backend-default" "Wrong X-Region should use fallback"

echo "✅ All method and header predicate routing tests passed"

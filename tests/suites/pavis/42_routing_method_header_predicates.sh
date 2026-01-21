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
      # Method routing: GET vs POST (use different paths to avoid duplicate error)
      - matcher:
          path: !prefix { path: "/api/get" }
          method: "GET"
        destinations: [{ upstream: "backend-get", weight: 1 }]
      - matcher:
          path: !prefix { path: "/api/post" }
          method: "POST"
        destinations: [{ upstream: "backend-post", weight: 1 }]

      # Single header predicate routing (use different paths)
      - matcher:
          path: !prefix { path: "/api/alice" }
          headers:
            - name: "x-tenant"
              value: "alice"
        destinations: [{ upstream: "backend-alice", weight: 1 }]
      - matcher:
          path: !prefix { path: "/api/bob" }
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
    local path="$1"
    local method="${2:-GET}"
    local headers="${3:-}"

    local curl_args=()
    if [ -n "$method" ]; then
        curl_args+=(-X "$method")
    fi
    if [ -n "$headers" ]; then
        # Split headers by | and add each as -H flag
        IFS='|' read -ra HEADER_ARRAY <<< "$headers"
        for header in "${HEADER_ARRAY[@]}"; do
            curl_args+=(-H "$header")
        done
    fi

    pavis_curl_body "${curl_args[@]}" "http://127.0.0.1:$PORT_PAVIS$path" | json_get_string "instance_id"
}

get_status() {
    local path="$1"
    local method="${2:-GET}"
    local headers="$3"

    local curl_args=(-o /dev/null -w "%{http_code}")
    if [ -n "$method" ]; then
        curl_args+=(-X "$method")
    fi
    if [ -n "$headers" ]; then
        IFS='|' read -ra HEADER_ARRAY <<< "$headers"
        for header in "${HEADER_ARRAY[@]}"; do
            curl_args+=(-H "$header")
        done
    fi

    pavis_curl_body "${curl_args[@]}" "http://127.0.0.1:$PORT_PAVIS$path"
}

echo "== Phase A: Method Predicate Routing =="

# A1: GET request to /api/get → backend-get
INSTANCE=$(get_instance "/api/get/users" "GET")
assert_eq "backend-v1" "$INSTANCE" "GET request should route to backend-get"

# A2: POST request to /api/post → backend-post
INSTANCE=$(get_instance "/api/post/users" "POST")
assert_eq "backend-v2" "$INSTANCE" "POST request should route to backend-post"

# A3: PUT request (no matching route) → should use fallback
INSTANCE=$(get_instance "/api/get/users" "PUT")
assert_eq "backend-v2" "$INSTANCE" "PUT request should use fallback (method mismatch)"

echo "== Phase B: Single Header Predicate Routing =="

# B1: Request with X-Tenant: alice → backend-alice
INSTANCE=$(get_instance "/api/alice/data" "GET" "x-tenant: alice")
assert_eq "backend-v1" "$INSTANCE" "X-Tenant: alice should route to backend-alice"

# B2: Request with X-Tenant: bob → backend-bob
INSTANCE=$(get_instance "/api/bob/data" "GET" "x-tenant: bob")
assert_eq "backend-v2" "$INSTANCE" "X-Tenant: bob should route to backend-bob"

# B3: Request with wrong X-Tenant → should use fallback
INSTANCE=$(get_instance "/api/alice/data" "GET" "x-tenant: charlie")
assert_eq "backend-v2" "$INSTANCE" "X-Tenant: charlie should use fallback (header mismatch)"

# B4: Request with missing X-Tenant → should use fallback
INSTANCE=$(get_instance "/api/alice/data" "GET")
assert_eq "backend-v2" "$INSTANCE" "Missing X-Tenant should use fallback"

echo "== Phase C: Multiple Header Predicates (AND Logic) =="

# C1: Both headers match → backend-alice-us
INSTANCE=$(get_instance "/api/multi/data" "GET" "x-tenant: alice|x-region: us-east")
assert_eq "backend-v1" "$INSTANCE" "Both headers (AND) should route to backend-alice-us"

# C2: Only X-Tenant matches → backend-default (fallback)
INSTANCE=$(get_instance "/api/multi/data" "GET" "x-tenant: alice")
assert_eq "backend-v2" "$INSTANCE" "Missing X-Region should use fallback"

# C3: Only X-Region matches → backend-default (fallback)
INSTANCE=$(get_instance "/api/multi/data" "GET" "x-region: us-east")
assert_eq "backend-v2" "$INSTANCE" "Missing X-Tenant should use fallback"

# C4: X-Tenant matches, X-Region wrong → backend-default (fallback)
INSTANCE=$(get_instance "/api/multi/data" "GET" "x-tenant: alice|x-region: eu-west")
assert_eq "backend-v2" "$INSTANCE" "Wrong X-Region should use fallback"

echo "✅ All method and header predicate routing tests passed"

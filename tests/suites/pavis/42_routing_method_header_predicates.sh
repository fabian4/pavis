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

PORT_METRICS=$(get_free_port)

cat <<-EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  level: debug
  metrics: "127.0.0.1:$PORT_METRICS"
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

echo "== P2 Extension: Multi-Method Routing =="

# Rewrite config to include P2 routes BEFORE fallback
cat <<-EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  level: debug
  metrics: "127.0.0.1:$PORT_METRICS"
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
      # P2: Multi-method list (OR semantics)
      - matcher:
          path: !prefix { path: "/api/multi-method" }
          methods: ["GET", "POST", "HEAD"]
        destinations: [{ upstream: "backend-get", weight: 1 }]

      # P2: Header prefix operator
      - matcher:
          path: !prefix { path: "/api/prefix" }
          headers:
            - operator: prefix
              name: "x-tenant"
              prefix: "team-"
        destinations: [{ upstream: "backend-alice", weight: 1 }]

      # P2: Header present operator
      - matcher:
          path: !prefix { path: "/api/present" }
          headers:
            - operator: present
              name: "x-debug"
        destinations: [{ upstream: "backend-get", weight: 1 }]

      # Fallback (Must be last)
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend-default", weight: 1 }]
EOF

# Regenerate config
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
# wait_for_reload is critical here as we just updated the config
wait_for_reload

# Test Multi-Method
# GET
INSTANCE=$(get_instance "/api/multi-method" "GET")
assert_eq "backend-v1" "$INSTANCE" "GET should match multi-method route"
# POST
INSTANCE=$(get_instance "/api/multi-method" "POST")
assert_eq "backend-v1" "$INSTANCE" "POST should match multi-method route"
# HEAD
STATUS=$(curl -s -I "http://127.0.0.1:$PORT_PAVIS/api/multi-method" | head -n 1 | awk '{print $2}')
assert_eq "200" "$STATUS" "HEAD should match multi-method route"

# PUT (not in list) -> fallback (backend-default v2)
INSTANCE=$(get_instance "/api/multi-method" "PUT")
assert_eq "backend-v2" "$INSTANCE" "PUT should NOT match multi-method route"


echo "== P2 Extension: Header Prefix Operator =="
# Matches "team-A" -> backend-alice (v1)
INSTANCE=$(get_instance "/api/prefix" "GET" "x-tenant: team-A")
assert_eq "backend-v1" "$INSTANCE" "Prefix match (team-A) should route to backend-alice"

# Matches "team-B" -> backend-alice (v1)
INSTANCE=$(get_instance "/api/prefix" "GET" "x-tenant: team-B")
assert_eq "backend-v1" "$INSTANCE" "Prefix match (team-B) should route to backend-alice"

# Does not match "other-team" -> fallback (v2)
INSTANCE=$(get_instance "/api/prefix" "GET" "x-tenant: other-team")
assert_eq "backend-v2" "$INSTANCE" "Non-prefix match should use fallback"


echo "== P2 Extension: Header Present Operator =="
# Present "true" -> backend-get (v1)
INSTANCE=$(get_instance "/api/present" "GET" "x-debug: true")
assert_eq "backend-v1" "$INSTANCE" "Present header should route to backend-get"

# Present "empty" (but present in request) -> backend-get (v1)
INSTANCE=$(get_instance "/api/present" "GET" "x-debug;")
assert_eq "backend-v1" "$INSTANCE" "Empty but present header should route to backend-get"

# Missing -> fallback (v2)
INSTANCE=$(get_instance "/api/present" "GET")
assert_eq "backend-v2" "$INSTANCE" "Missing header should use fallback"

echo "== P2 Extension: Header Absent Operator =="

# Reload config with absent operator route
cat <<-EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  level: debug
  metrics: "127.0.0.1:$PORT_METRICS"
upstreams:
  - name: "backend-get"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
  - name: "backend-external"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
  - name: "backend-default"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]
routes:
  - host: "*"
    paths:
      # P2: Header absent operator (external traffic only)
      - matcher:
          path: !prefix { path: "/api/external" }
          headers:
            - operator: absent
              name: "x-internal"
        destinations: [{ upstream: "backend-external", weight: 1 }]
      # Fallback (must be last)
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend-default", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
wait_for_reload

# Test absent match (header missing)
INSTANCE=$(get_instance "/api/external" "GET")
assert_eq "backend-v1" "$INSTANCE" "Missing header should match absent operator"

# Test absent non-match (header present)
INSTANCE=$(get_instance "/api/external" "GET" "x-internal: true")
assert_eq "backend-v2" "$INSTANCE" "Present header should NOT match absent operator"

echo "✓ Header absent operator test passed"

echo "== P2 Metrics Verification =="

# Reload config with metrics enabled
cat <<-EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  level: debug
  metrics: "127.0.0.1:$PORT_METRICS"
upstreams:
  - name: "backend-get"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
  - name: "backend-external"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
  - name: "backend-alice"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
  - name: "backend-default"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/api/prefix" }
          headers:
            - operator: prefix
              name: "x-tenant"
              prefix: "team-"
        destinations: [{ upstream: "backend-alice", weight: 1 }]
      - matcher:
          path: !prefix { path: "/api/present" }
          headers:
            - operator: present
              name: "x-debug"
        destinations: [{ upstream: "backend-get", weight: 1 }]
      - matcher:
          path: !prefix { path: "/api/external" }
          headers:
            - operator: absent
              name: "x-internal"
        destinations: [{ upstream: "backend-external", weight: 1 }]
      - matcher:
          path: !prefix { path: "/api/exact" }
          headers:
            - name: "x-tenant"
              value: "alice"
        destinations: [{ upstream: "backend-alice", weight: 1 }]
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend-default", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
# Give it a bit more time to reload and bind metrics port
sleep 2
wait_for_port "$PORT_METRICS" 10

# Trigger requests to populate metrics
echo "Triggering requests for metrics..."
get_instance "/api/exact" "GET" "x-tenant: alice" >/dev/null
get_instance "/api/prefix" "GET" "x-tenant: team-alpha" >/dev/null
get_instance "/api/present" "GET" "x-debug: true" >/dev/null
get_instance "/api/external" "GET" >/dev/null

# Verify operator-specific counters
echo "Verifying operator metrics..."
METRICS_URL="http://127.0.0.1:$PORT_METRICS/metrics"

# Check exact evaluations
assert_metric_at_least 'pavis_route_match_predicate_evaluations_total\{.*operator="exact".*\}' 1 10 "$METRICS_URL"
# Check prefix evaluations
assert_metric_at_least 'pavis_route_match_predicate_evaluations_total\{.*operator="prefix".*\}' 1 10 "$METRICS_URL"
# Check present evaluations
assert_metric_at_least 'pavis_route_match_predicate_evaluations_total\{.*operator="present".*\}' 1 10 "$METRICS_URL"
# Check absent evaluations
assert_metric_at_least 'pavis_route_match_predicate_evaluations_total\{.*operator="absent".*\}' 1 10 "$METRICS_URL"

# Check match attempts
assert_metric_at_least 'pavis_route_match_attempts_total' 1 10 "$METRICS_URL"

echo "✓ P2 metrics verification passed"

echo "✅ All method and header predicate routing tests passed"

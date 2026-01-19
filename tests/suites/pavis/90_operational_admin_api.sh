#!/bin/bash
set -e

# Case: operational_admin_api
# Category: Operational Lifecycle (Phase 7)
# Invariants: Admin API endpoints provide read-only introspection

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "operational_admin_api"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_ADMIN=$(get_free_port)

# 1. Define Config with Admin API Enabled
cat <<EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend1"
    endpoints:
      - ip: "127.0.0.1"
        port: ${UPSTREAM_HTTP_PORT_V1}
  - name: "backend2"
    endpoints:
      - ip: "127.0.0.1"
        port: ${UPSTREAM_HTTP_PORT_V2}
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/api"
        destinations:
          - upstream: "backend1"
            weight: 1
      - matcher: !prefix
          path: "/v2"
        destinations:
          - upstream: "backend2"
            weight: 1
admin:
  enabled: true
  address: "127.0.0.1:$PORT_ADMIN"
shutdown:
  enabled: false  # Disable for test speed
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# 2. Start Pavis with Admin API
run_pavis "$TEST_TMP/config.pvs" ""

# 3. Wait for Both Listeners
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_ADMIN" 5

echo "✓ Pavis started with admin API on port $PORT_ADMIN"

# 4. Test /health Endpoint
health_response=$(pavis_curl_body "http://127.0.0.1:$PORT_ADMIN/health")
echo "Health response: $health_response"

# Verify health endpoint returns correct JSON
echo "$health_response" | assert_json_has_key "status"
health_status=$(echo "$health_response" | json_get_string "status")

if [ "$health_status" != "healthy" ]; then
    echo "❌ Expected status 'healthy', got '$health_status'"
    exit 1
fi

echo "✓ /health endpoint returns correct status"

# 5. Test /stats Endpoint
stats_response=$(pavis_curl_body "http://127.0.0.1:$PORT_ADMIN/stats")
echo "Stats response: $stats_response"

# Verify stats endpoint returns expected fields
echo "$stats_response" | assert_json_has_key "version"
echo "$stats_response" | assert_json_has_key "uptime_seconds"
echo "$stats_response" | assert_json_has_key "listeners"
echo "$stats_response" | assert_json_has_key "upstreams"
echo "$stats_response" | assert_json_has_key "routes"

admin_version=$(get_admin_version "http://127.0.0.1:$PORT_ADMIN")
echo "Admin version: $admin_version"

echo "✓ /stats endpoint returns all required fields"

# 6. Verify Config Counts in Stats
listeners_count=$(echo "$stats_response" | json_get_number "listeners")
upstreams_count=$(echo "$stats_response" | json_get_number "upstreams")
routes_count=$(echo "$stats_response" | json_get_number "routes")

if [ "$listeners_count" != "1" ]; then
    echo "❌ Expected 1 listener, got $listeners_count"
    exit 1
fi

if [ "$upstreams_count" != "2" ]; then
    echo "❌ Expected 2 upstreams, got $upstreams_count"
    exit 1
fi

if [ "$routes_count" != "2" ]; then
    echo "❌ Expected 2 routes, got $routes_count"
    exit 1
fi

echo "✓ /stats endpoint reflects correct config counts"

# 7. Verify Uptime Increases
sleep 2
stats_response_2=$(pavis_curl_body "http://127.0.0.1:$PORT_ADMIN/stats")
uptime_1=$(echo "$stats_response" | json_get_number "uptime_seconds")
uptime_2=$(echo "$stats_response_2" | json_get_number "uptime_seconds")

if [ "$uptime_2" -le "$uptime_1" ]; then
    echo "❌ Expected uptime to increase, got $uptime_1 -> $uptime_2"
    exit 1
fi

echo "✓ Uptime counter increases correctly"

# 8. Test 404 for Unknown Path
unknown_response=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT_ADMIN/unknown")
if [ "$unknown_response" != "404" ]; then
    echo "❌ Expected 404 for unknown path, got $unknown_response"
    exit 1
fi

echo "✓ Unknown paths return 404"

# 9. Verify Admin API Does Not Affect Traffic Routing
traffic_response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/api/echo")
echo "$traffic_response" | assert_json_has_key "instance_id"

echo "✓ Traffic routing unaffected by admin API"

# 10. Verify Admin API is Isolated (Not on Traffic Port)
admin_on_traffic=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT_PAVIS/health" || echo "000")
if [ "$admin_on_traffic" = "200" ]; then
    echo "❌ Admin API should not be accessible on traffic port"
    exit 1
fi

echo "✓ Admin API isolated to admin port only"

echo "✅ operational_admin_api passed"

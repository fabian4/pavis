#!/bin/bash
set -e

# Case: traffic_40_routing_semantics
# Category: Traffic Management
# Invariants: C (Atomic Switch), D (Zero-Option)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "traffic_40"
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
  - name: "backend-v1"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
  - name: "backend-v2"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]
routes:
  - host: "*"
    paths:
      - matcher: !exact { path: "/exact" }
        destinations: [{ upstream: "backend-v2", weight: 1 }]
      - matcher: !prefix { path: "/prefix" }
        destinations: [{ upstream: "backend-v1", weight: 1 }]
      - matcher: !regex { path: '^/regex/[0-9]+$' }
        destinations: [{ upstream: "backend-v2", weight: 1 }]
      - matcher: !prefix { path: "/headers" }
        request_headers:
          set_headers:
            - ["X-Request-Set", "pavis-set"]
          append_headers:
            - ["X-Request-Append", "pavis-appended"]
          add_headers:
            - ["X-Request-Add", "pavis-added"]
          remove_headers:
            - "X-To-Remove"
        response_headers:
          set_headers:
            - ["X-Response-Set", "pavis-resp-set"]
          remove_headers:
            - "X-Internal-Header"
        destinations: [{ upstream: "backend-v1", weight: 1 }]
      - matcher: !exact { path: "/redirect-me" }
        status: 301
        location: "http://example.com/new-location"
      - matcher: !exact { path: "/direct-me" }
        status: 200
        body: "Custom Static Response"
      - matcher: !prefix { path: "/" }
        destinations: [{ upstream: "backend-v1", weight: 1 }]
  - host: "rewrite.test"
    paths:
      - matcher: !prefix { path: "/service-a" }
        rewrite:
          path: ""
          host: "rewritten.internal"
        destinations: [{ upstream: "backend-v1", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
cp "$TEST_TMP/config.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

get_instance() {
    pavis_curl_body "http://127.0.0.1:$PORT_PAVIS$1" |
        python3 -c "import sys, json; print(json.load(sys.stdin).get('instance_id',''))"
}

# Step 1: Prefix vs Exact precedence
if [ "$(get_instance "/exact")" != "backend-v2" ]; then
    echo "❌ Exact route did not win over prefix"
    exit 1
fi
if [ "$(get_instance "/prefix/foo")" != "backend-v1" ]; then
    echo "❌ Prefix route did not route to backend-v1"
    exit 1
fi
if [ "$(get_instance "/anything")" != "backend-v1" ]; then
    echo "❌ Fallback prefix route failed"
    exit 1
fi

# Step 2: Regex routing
if [ "$(get_instance "/regex/123")" != "backend-v2" ]; then
    echo "❌ Regex route did not match digits"
    exit 1
fi
if [ "$(get_instance "/regex/abc")" != "backend-v1" ]; then
    echo "❌ Regex miss did not fall back"
    exit 1
fi

# Step 3: Header policies
response=$(curl -s -H "X-To-Remove: should-be-gone" -H "X-Request-Append: original" "http://127.0.0.1:$PORT_PAVIS/headers/echo")
val=$(echo "$response" | python3 -c "import sys,json; print(json.load(sys.stdin)['headers'].get('x-request-set',[''])[0])")
assert_eq "pavis-set" "$val" "X-Request-Set should be set"
val=$(echo "$response" | python3 -c "import sys,json; h=json.load(sys.stdin)['headers']; print(', '.join(h.get('x-request-append',[])))")
assert_eq "original, pavis-appended" "$val" "X-Request-Append should be appended"
val=$(echo "$response" | python3 -c "import sys,json; print(json.load(sys.stdin)['headers'].get('x-request-add',[''])[0])")
assert_eq "pavis-added" "$val" "X-Request-Add should be added"
val=$(echo "$response" | python3 -c "import sys,json; print('PRESENT' if 'x-to-remove' in json.load(sys.stdin)['headers'] else 'ABSENT')")
assert_eq "ABSENT" "$val" "X-To-Remove should be stripped"
resp_headers=$(curl -sI "http://127.0.0.1:$PORT_PAVIS/headers/echo")
if ! echo "$resp_headers" | grep -qi "X-Response-Set: pavis-resp-set"; then
    echo "❌ X-Response-Set missing"
    exit 1
fi

# Step 4: Redirect action
redirect_headers=$(curl -sI "http://127.0.0.1:$PORT_PAVIS/redirect-me")
status=$(echo "$redirect_headers" | head -n 1 | awk '{print $2}')
assert_eq "301" "$status" "Redirect status"
location=$(echo "$redirect_headers" | awk '/[Ll]ocation:/ {print $2}' | tr -d '\r')
assert_eq "http://example.com/new-location" "$location" "Redirect location"

# Step 5: Direct response
body=$(curl -s "http://127.0.0.1:$PORT_PAVIS/direct-me")
assert_eq "Custom Static Response" "$body" "Direct response body"
status=$(curl -sI "http://127.0.0.1:$PORT_PAVIS/direct-me" | head -n1 | awk '{print $2}')
assert_eq "200" "$status" "Direct response status"

# Step 6: Rewrite path & host
rewrite_resp=$(curl -s -H "Host: rewrite.test" "http://127.0.0.1:$PORT_PAVIS/service-a/echo?q=bar")
rewritten_path=$(echo "$rewrite_resp" | python3 -c "import sys,json; print(json.load(sys.stdin)['path'])")
assert_eq "/echo" "$rewritten_path" "Path should be rewritten"
rewritten_query=$(echo "$rewrite_resp" | python3 -c "import sys,json; print(json.load(sys.stdin)['query'])")
assert_eq "q=bar" "$rewritten_query" "Query must be preserved"
rewritten_host=$(echo "$rewrite_resp" | python3 -c "import sys,json; print(json.load(sys.stdin)['headers'].get('host',[''])[0])")
assert_eq "rewritten.internal" "$rewritten_host" "Host should be rewritten"

echo "✅ traffic_10_routing_semantics passed"
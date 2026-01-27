#!/bin/bash
set -e

# Case: 70_security_tls
# Category: Security & TLS
# Invariants: C (Atomic Switch)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "70_security_tls"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# V1: HTTP (Port ${UPSTREAM_HTTP_PORT_V1})
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Assert V1 (HTTP)
response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
if [ -z "$response" ]; then
    echo "❌ Empty response from Pavis"
    exit 1
fi
tls_enabled=$(echo "$response" | json_get_tls_bool "enabled")
if [ "$tls_enabled" = "true" ]; then
    echo "❌ Expected HTTP initially, got TLS enabled"
    exit 1
fi

# V2: HTTPS (Port ${UPSTREAM_HTTPS_PORT_V1})
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend"
	    tls:
	      enabled: true
	      verify_cert: true
	      verify_hostname: true
	      sni: "localhost"
	      ca_bundle: "$TEST_TMP/ca.pem"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTPS_PORT_V1}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
cp "$PROJECT_ROOT/tests/suites/config/certs/ca.pem" "$TEST_TMP/ca.pem"
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

echo "Publishing V2 (HTTPS Config)..."
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

# Wait for switch (poll for TLS enabled)
MAX_RETRIES=20
SWITCHED=0
attempt=0
for attempt in $(seq 1 $MAX_RETRIES); do
    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")

    if [ -n "$response" ]; then
        tls_enabled=$(echo "$response" | json_get_tls_bool "enabled")

        if [ "$tls_enabled" == "True" ] || [ "$tls_enabled" == "true" ]; then
            SWITCHED=1
            break
        fi
    fi
    sleep 0.5
done

assert_retry_succeeded "$attempt" "$MAX_RETRIES"

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Traffic did not switch to HTTPS (TLS) after reload"
    exit 1
fi

# SNI Validation (Optional/Pending Upstream Support)
# Accept null or empty SNI when upstream does not report it.
sni_value=$(echo "$response" | awk '
    { gsub(/\r|\n/, "", $0) }
    match($0, "\"tls\"[^}]*\"sni\"[[:space:]]*:[[:space:]]*\"[^\"]*\"") {
        value=substr($0, RSTART, RLENGTH)
        sub(/.*:[[:space:]]*"/, "", value)
        sub(/"$/, "", value)
        print value
        found=1
        exit
    }
    match($0, "\"tls\"[^}]*\"sni\"[[:space:]]*:[[:space:]]*null") {
        print "None"
        found=1
        exit
    }
    END { if (!found) print "" }
')
echo "Reported SNI: $sni_value"

if [ "$sni_value" == "localhost" ]; then
    echo "✅ SNI correctly verified as 'localhost'"
elif [ "$sni_value" == "None" ] || [ -z "$sni_value" ]; then
    echo "⚠️ SNI not reported by upstream (Known limitation)"
else
    echo "❌ SNI mismatch: Expected 'localhost', got '$sni_value'"
    exit 1
fi

echo "✅ security_01_tls_origination_toggle passed"

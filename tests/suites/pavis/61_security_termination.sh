#!/bin/bash
set -e

# Case: security_02_termination
# Category: Security & TLS
# Description: Verifies Server-side TLS termination and mTLS.

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "security_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

# 1. Generate Certificates
echo "🔑 Generating certificates..."
mkdir -p "$TEST_TMP/certs"
# CA
openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout "$TEST_TMP/certs/ca.key" \
    -out "$TEST_TMP/certs/ca.pem" \
    -subj "/CN=PavisTestCA" -days 365 2>/dev/null

# Server Cert
openssl req -newkey rsa:2048 -nodes \
    -keyout "$TEST_TMP/certs/server.key" \
    -out "$TEST_TMP/certs/server.csr" \
    -subj "/CN=localhost" 2>/dev/null
openssl x509 -req -in "$TEST_TMP/certs/server.csr" \
    -CA "$TEST_TMP/certs/ca.pem" -CAkey "$TEST_TMP/certs/ca.key" -CAcreateserial \
    -out "$TEST_TMP/certs/server.pem" -days 365 2>/dev/null

# Client Cert
openssl req -newkey rsa:2048 -nodes \
    -keyout "$TEST_TMP/certs/client.key" \
    -out "$TEST_TMP/certs/client.csr" \
    -subj "/CN=client" 2>/dev/null
openssl x509 -req -in "$TEST_TMP/certs/client.csr" \
    -CA "$TEST_TMP/certs/ca.pem" -CAkey "$TEST_TMP/certs/ca.key" -CAcreateserial \
    -out "$TEST_TMP/certs/client.pem" -days 365 2>/dev/null

# 2. Start Mock Relay
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# 3. Config: TLS Termination with Required Client Auth
cat <<EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "https"
    address: "127.0.0.1:$PORT_PAVIS"
    tls:
      cert_path: "$TEST_TMP/certs/server.pem"
      key_path: "$TEST_TMP/certs/server.key"
      client_auth: !required
        ca_path: "$TEST_TMP/certs/ca.pem"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        destinations:
          - upstream: "backend"
            weight: 1
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# 4. Start Pavis
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"

# I need to wait for the port to be open.
wait_for_port "$PORT_PAVIS" 10

# 5. Assert TLS Termination
echo "--- Testing TLS Termination (HTTPS) ---"
# Test without client cert (should fail)
set +e
response_code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 \
    --cacert "$TEST_TMP/certs/ca.pem" \
    --resolve "localhost:$PORT_PAVIS:127.0.0.1" \
    "https://localhost:$PORT_PAVIS/healthz")
set -e
if [ "$response_code" == "200" ]; then
    echo "❌ Request succeeded without client certificate, but it should be required"
    exit 1
fi

# Test with client cert (should succeed)
echo "--- Testing mTLS ---"
response=$(curl -sS --max-time 5 --cacert "$TEST_TMP/certs/ca.pem" \
    --resolve "localhost:$PORT_PAVIS:127.0.0.1" \
    --cert "$TEST_TMP/certs/client.pem" \
    --key "$TEST_TMP/certs/client.key" \
    "https://localhost:$PORT_PAVIS/echo")

instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['instance_id'])")
assert_eq "backend-v1" "$instance" "Should succeed with valid client certificate"

echo "✅ security_02_termination passed"

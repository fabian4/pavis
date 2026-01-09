#!/bin/bash
set -e

# Case 13: Outbound mTLS
# Verifies that Pavis can connect to an upstream that requires client certificates.

echo "⏭️ Skipping Case 13 (Outbound mTLS marked TODO in roadmap)"
exit 0

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "integrated_13"
cleanup_trap() { [ -n "$UPSTREAM_PID" ] && kill "$UPSTREAM_PID" 2>/dev/null || true; cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)
PORT_PAVIS=$(get_free_port)
PORT_UPSTREAM=$(get_free_port)

# 1. Generate Certificates
mkdir -p "$TEST_TMP/certs"
# CA for upstream to verify Pavis
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$TEST_TMP/certs/ca.key" -out "$TEST_TMP/certs/ca.crt" -subj "/CN=Test CA" -days 1 2>/dev/null
# Upstream Server Cert
openssl req -newkey rsa:2048 -nodes -keyout "$TEST_TMP/certs/server.key" -out "$TEST_TMP/certs/server.csr" -subj "/CN=127.0.0.1" 2>/dev/null
openssl x509 -req -in "$TEST_TMP/certs/server.csr" -CA "$TEST_TMP/certs/ca.crt" -CAkey "$TEST_TMP/certs/ca.key" -CAcreateserial -out "$TEST_TMP/certs/server.crt" -days 1 2>/dev/null
# Pavis Client Cert
openssl req -newkey rsa:2048 -nodes -keyout "$TEST_TMP/certs/client.key" -out "$TEST_TMP/certs/client.csr" -subj "/CN=pavis-client" 2>/dev/null
openssl x509 -req -in "$TEST_TMP/certs/client.csr" -CA "$TEST_TMP/certs/ca.crt" -CAkey "$TEST_TMP/certs/ca.key" -CAcreateserial -out "$TEST_TMP/certs/client.crt" -days 1 2>/dev/null

# 2. Start mTLS Upstream (using openssl s_server)
# -Verify 1: request a certificate from the client and fail if not provided
openssl s_server -cert "$TEST_TMP/certs/server.crt" -key "$TEST_TMP/certs/server.key" \
    -CAfile "$TEST_TMP/certs/ca.crt" -Verify 1 \
    -accept $PORT_UPSTREAM -www -quiet >/dev/null 2>&1 &
UPSTREAM_PID=$!
wait_for_port $PORT_UPSTREAM 5

# 3. Start Relay
mkdir -p "$TEST_TMP/storage"
cat <<-EOF > "$TEST_TMP/relay.yaml"
	identity:
	  name: "integrated-13"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  root_dir: "$TEST_TMP/storage"
	artifact:
	  lkg_path: "$TEST_TMP/storage/lkg.pvs"
	pipeline:
	  ingest:
	    source:
	      kind: file
	      path: "$TEST_TMP/ingest.yaml"
EOF

cat <<-EOF > "$TEST_TMP/ingest.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "mtls-backend"
	    tls:
	      enabled: true
	      verify_cert: false
	      verify_hostname: false
	      cert:
	        cert_path: "$TEST_TMP/certs/client.crt"
	        key_path: "$TEST_TMP/certs/client.key"
	    endpoints:
	      - address: "127.0.0.1"
	        port: $PORT_UPSTREAM
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix
	          path: "/"
	        destinations:
	          - upstream: "mtls-backend"
	            weight: 1
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 4. Start Pavis
gen_pvs "$TEST_TMP/ingest.yaml" "$TEST_TMP/boot.pvs"
run_pavis "$TEST_TMP/boot.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

# 5. Assertion
RESP=$(curl -s -i "http://127.0.0.1:$PORT_PAVIS")
if ! echo "$RESP" | grep -q "200 OK"; then
    echo "❌ Outbound mTLS request failed"
    echo "$RESP"
    exit 1
fi

echo "✅ Case 13_outbound_mtls passed"
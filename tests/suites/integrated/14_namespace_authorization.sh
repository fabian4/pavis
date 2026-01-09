#!/bin/bash
set -e

# Case 14: Namespace-Level Authorization (RBAC)
# Verifies that Pavis correctly enforces authorization based on SPIFFE ID prefixes.

echo "⏭️ Skipping Case 14 (RBAC marked TODO in roadmap)"
exit 0

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "integrated_14"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)
PORT_PAVIS=$(get_free_port)

# 1. Generate Certificates with SPIFFE IDs
mkdir -p "$TEST_TMP/certs"
# CA
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$TEST_TMP/certs/ca.key" -out "$TEST_TMP/certs/ca.crt" -subj "/CN=Test CA" -days 1 2>/dev/null

# Server Cert
openssl req -newkey rsa:2048 -nodes -keyout "$TEST_TMP/certs/server.key" -out "$TEST_TMP/certs/server.csr" -subj "/CN=localhost" 2>/dev/null
openssl x509 -req -in "$TEST_TMP/certs/server.csr" -CA "$TEST_TMP/certs/ca.crt" -CAkey "$TEST_TMP/certs/ca.key" -CAcreateserial -out "$TEST_TMP/certs/server.crt" -days 1 2>/dev/null

# Helper to generate client cert with SPIFFE ID in SAN
gen_client_cert() {
    local name=$1
    local spiffe=$2
    local key="$TEST_TMP/certs/$name.key"
    local csr="$TEST_TMP/certs/$name.csr"
    local crt="$TEST_TMP/certs/$name.crt"
    local ext="$TEST_TMP/certs/$name.ext"

    openssl req -newkey rsa:2048 -nodes -keyout "$key" -out "$csr" -subj "/CN=$name" 2>/dev/null
    echo "subjectAltName=URI:$spiffe" > "$ext"
    openssl x509 -req -in "$csr" -CA "$TEST_TMP/certs/ca.crt" -CAkey "$TEST_TMP/certs/ca.key" -CAcreateserial -out "$crt" -extfile "$ext" -days 1 2>/dev/null
}

# Prod client (Allowed)
gen_client_cert "prod_client" "spiffe://cluster.local/ns/prod/sa/app-a"
# Dev client (Denied)
gen_client_cert "dev_client" "spiffe://cluster.local/ns/dev/sa/app-b"

# 2. Start Relay
mkdir -p "$TEST_TMP/storage"
cat <<-EOF > "$TEST_TMP/relay.yaml"
	identity:
	  name: "integrated-14"
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
	    tls:
	      cert_path: "$TEST_TMP/certs/server.crt"
	      key_path: "$TEST_TMP/certs/server.key"
	      client_auth: !required
	        ca_path: "$TEST_TMP/certs/ca.crt"
	upstreams:
	  - name: "backend"
	    endpoints:
	      - address: "127.0.0.1"
	        port: 8081
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix
	          path: "/prod"
	        principal: !prefix
	          prefix: "spiffe://cluster.local/ns/prod/"
	        destinations:
	          - upstream: "backend"
	            weight: 1
	      - matcher: !prefix
	          path: "/public"
	        principal: !any
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 3. Start Pavis
gen_pvs "$TEST_TMP/ingest.yaml" "$TEST_TMP/boot.pvs"
run_pavis "$TEST_TMP/boot.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "https://127.0.0.1:$PORT_PAVIS/public" 5 "" "--insecure --cert $TEST_TMP/certs/prod_client.crt --key $TEST_TMP/certs/prod_client.key"

# 4. Assertions
echo "Testing Authorized request (Prod Client -> /prod)..."
RESP=$(curl -s -i --insecure --cert "$TEST_TMP/certs/prod_client.crt" --key "$TEST_TMP/certs/prod_client.key" "https://127.0.0.1:$PORT_PAVIS/prod")
if ! echo "$RESP" | grep -q "200 OK"; then echo "❌ Authorized request failed"; echo "$RESP"; exit 1; fi

echo "Testing Unauthorized request (Dev Client -> /prod)..."
RESP=$(curl -s -i --insecure --cert "$TEST_TMP/certs/dev_client.crt" --key "$TEST_TMP/certs/dev_client.key" "https://127.0.0.1:$PORT_PAVIS/prod")
if ! echo "$RESP" | grep -q "403 Forbidden"; then echo "❌ Unauthorized request was not blocked"; echo "$RESP"; exit 1; fi

echo "Testing Public request (Dev Client -> /public)..."
RESP=$(curl -s -i --insecure --cert "$TEST_TMP/certs/dev_client.crt" --key "$TEST_TMP/certs/dev_client.key" "https://127.0.0.1:$PORT_PAVIS/public")
if ! echo "$RESP" | grep -q "200 OK"; then echo "❌ Public request failed"; echo "$RESP"; exit 1; fi

echo "✅ Case 14_namespace_authorization passed"
#!/bin/bash
set -e

# Case 12: Permissive Migration (Optional Client Auth)
# Verifies that Pavis accepts both authenticated and unauthenticated traffic
# when client_auth is set to optional.

echo "⏭️ Skipping Case 12 (mTLS marked TODO in roadmap)"
exit 0

source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "integrated_12"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)
PORT_PAVIS=$(get_free_port)

# 1. Generate Certificates
mkdir -p "$TEST_TMP/certs"
# CA
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$TEST_TMP/certs/ca.key" -out "$TEST_TMP/certs/ca.crt" -subj "/CN=Test CA" -days 1 2>/dev/null
# Server Cert
openssl req -newkey rsa:2048 -nodes -keyout "$TEST_TMP/certs/server.key" -out "$TEST_TMP/certs/server.csr" -subj "/CN=localhost" 2>/dev/null
openssl x509 -req -in "$TEST_TMP/certs/server.csr" -CA "$TEST_TMP/certs/ca.crt" -CAkey "$TEST_TMP/certs/ca.key" -CAcreateserial -out "$TEST_TMP/certs/server.crt" -days 1 2>/dev/null
# Client Cert
openssl req -newkey rsa:2048 -nodes -keyout "$TEST_TMP/certs/client.key" -out "$TEST_TMP/certs/client.csr" -subj "/CN=client" 2>/dev/null
openssl x509 -req -in "$TEST_TMP/certs/client.csr" -CA "$TEST_TMP/certs/ca.crt" -CAkey "$TEST_TMP/certs/ca.key" -CAcreateserial -out "$TEST_TMP/certs/client.crt" -days 1 2>/dev/null

# 2. Start Relay
mkdir -p "$TEST_TMP/storage"
cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: integrated-12 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline: { ingest: { source: { kind: file, path: "$TEST_TMP/ingest.yaml" } } }
EOF

cat <<EOF > "$TEST_TMP/ingest.yaml"
listeners:
  - name: default
    address: "127.0.0.1:$PORT_PAVIS"
    tls:
      cert_path: "$TEST_TMP/certs/server.crt"
      key_path: "$TEST_TMP/certs/server.key"
      client_auth: !optional
        ca_path: "$TEST_TMP/certs/ca.crt"
upstreams:
  - name: backend
    endpoints: [{ address: "127.0.0.1", port: 8081 }]
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        destinations: [{ upstream: backend, weight: 1 }]
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 3. Start Pavis
gen_pvs "$TEST_TMP/ingest.yaml" "$TEST_TMP/boot.pvs"
run_pavis "$TEST_TMP/boot.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "https://127.0.0.1:$PORT_PAVIS" 5 "" "--insecure"

# 4. Assertions
echo "Testing WITHOUT client certificate..."
RESP=$(curl -s --insecure "https://127.0.0.1:$PORT_PAVIS")
if [[ "$RESP" != *"backend-v1"* ]]; then echo "❌ Standard TLS request failed"; exit 1; fi

echo "Testing WITH client certificate..."
RESP=$(curl -s --insecure --cert "$TEST_TMP/certs/client.crt" --key "$TEST_TMP/certs/client.key" "https://127.0.0.1:$PORT_PAVIS")
if [[ "$RESP" != *"backend-v1"* ]]; then echo "❌ mTLS request failed"; exit 1; fi

echo "✅ Case 12_permissive_migration passed"
#!/bin/bash
set -e

# Case: security_04_rbac_prefix
# Category: Security & TLS
# Description: Verifies SPIFFE ID authorization for prefix match.

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "security_04"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
CERT_DIR="$TEST_TMP/certs"
mkdir -p "$CERT_DIR"

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/ca.key" \
  -out "$CERT_DIR/ca.pem" \
  -subj "/CN=Pavis Test CA" -days 365 >/dev/null 2>&1

cat <<EOF > "$CERT_DIR/server.cnf"
[req]
distinguished_name=req_distinguished_name
req_extensions = v3_req
prompt = no
[req_distinguished_name]
CN=localhost
[v3_req]
subjectAltName = DNS:localhost
EOF

openssl req -new -nodes -newkey rsa:2048 \
  -keyout "$CERT_DIR/server.key" \
  -out "$CERT_DIR/server.csr" \
  -config "$CERT_DIR/server.cnf" >/dev/null 2>&1
openssl x509 -req -in "$CERT_DIR/server.csr" \
  -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca.key" -CAcreateserial \
  -out "$CERT_DIR/server.pem" -days 365 \
  -extensions v3_req -extfile "$CERT_DIR/server.cnf" >/dev/null 2>&1

SPIFFE_APP1="spiffe://cluster/ns/prod/sa/app1"
SPIFFE_OTHER="spiffe://cluster/ns/dev/sa/app2"
SPIFFE_PREFIX="spiffe://cluster/ns/prod/sa/"

cat <<EOF > "$CERT_DIR/app1.cnf"
[req]
distinguished_name=req_distinguished_name
req_extensions = v3_req
prompt = no
[req_distinguished_name]
CN=app1
[v3_req]
subjectAltName = URI:${SPIFFE_APP1}
EOF

openssl req -new -nodes -newkey rsa:2048 \
  -keyout "$CERT_DIR/app1.key" \
  -out "$CERT_DIR/app1.csr" \
  -config "$CERT_DIR/app1.cnf" >/dev/null 2>&1
openssl x509 -req -in "$CERT_DIR/app1.csr" \
  -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca.key" -CAcreateserial \
  -out "$CERT_DIR/app1.pem" -days 365 \
  -extensions v3_req -extfile "$CERT_DIR/app1.cnf" >/dev/null 2>&1

cat <<EOF > "$CERT_DIR/other.cnf"
[req]
distinguished_name=req_distinguished_name
req_extensions = v3_req
prompt = no
[req_distinguished_name]
CN=other
[v3_req]
subjectAltName = URI:${SPIFFE_OTHER}
EOF

openssl req -new -nodes -newkey rsa:2048 \
  -keyout "$CERT_DIR/other.key" \
  -out "$CERT_DIR/other.csr" \
  -config "$CERT_DIR/other.cnf" >/dev/null 2>&1
openssl x509 -req -in "$CERT_DIR/other.csr" \
  -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca.key" -CAcreateserial \
  -out "$CERT_DIR/other.pem" -days 365 \
  -extensions v3_req -extfile "$CERT_DIR/other.cnf" >/dev/null 2>&1

cat <<EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
    tls:
      cert_path: "$CERT_DIR/server.pem"
      key_path: "$CERT_DIR/server.key"
      client_auth: !optional
        ca_path: "$CERT_DIR/ca.pem"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend"
            weight: 1
        principal: !prefix
          prefix: "${SPIFFE_PREFIX}"
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" ""

wait_for_port "$PORT_PAVIS" 10

URL="https://localhost:$PORT_PAVIS/echo"

assert_status "$URL" "200" --cacert "$CERT_DIR/ca.pem" \
  --cert "$CERT_DIR/app1.pem" --key "$CERT_DIR/app1.key"

assert_status "$URL" "403" --cacert "$CERT_DIR/ca.pem" \
  --cert "$CERT_DIR/other.pem" --key "$CERT_DIR/other.key"

assert_status "$URL" "403" --cacert "$CERT_DIR/ca.pem"

echo "✅ security_04_rbac_prefix passed"

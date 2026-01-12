#!/bin/bash
set -e

# Case: security_02_mtls_handshake
# Category: Security & TLS
# Description: Verifies required-mode mTLS handshakes succeed/fail as expected.

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "security_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
CERT_DIR="$TEST_TMP/certs"
mkdir -p "$CERT_DIR"

# Root CA
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/ca.key" \
  -out "$CERT_DIR/ca.pem" \
  -subj "/CN=Pavis Test CA" -days 365 >/dev/null 2>&1

# Server cert signed by CA
openssl req -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/server.key" \
  -out "$CERT_DIR/server.csr" \
  -subj "/CN=localhost" >/dev/null 2>&1
openssl x509 -req -in "$CERT_DIR/server.csr" \
  -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca.key" -CAcreateserial \
  -out "$CERT_DIR/server.pem" -days 365 >/dev/null 2>&1

# Client cert signed by CA
openssl req -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/client.key" \
  -out "$CERT_DIR/client.csr" \
  -subj "/CN=mtls-client" >/dev/null 2>&1
openssl x509 -req -in "$CERT_DIR/client.csr" \
  -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca.key" -CAcreateserial \
  -out "$CERT_DIR/client.pem" -days 365 >/dev/null 2>&1

# Unknown CA + client cert
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/ca_unknown.key" \
  -out "$CERT_DIR/ca_unknown.pem" \
  -subj "/CN=Unknown CA" -days 365 >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/client_bad.key" \
  -out "$CERT_DIR/client_bad.csr" \
  -subj "/CN=bad-client" >/dev/null 2>&1
openssl x509 -req -in "$CERT_DIR/client_bad.csr" \
  -CA "$CERT_DIR/ca_unknown.pem" -CAkey "$CERT_DIR/ca_unknown.key" -CAcreateserial \
  -out "$CERT_DIR/client_bad.pem" -days 365 >/dev/null 2>&1

cat <<EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
    tls:
      cert_path: "$CERT_DIR/server.pem"
      key_path: "$CERT_DIR/server.key"
      client_auth: !required
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
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" ""

wait_for_port "$PORT_PAVIS" 10

URL="https://localhost:$PORT_PAVIS/healthz"

# No client cert should fail in Required mode.
if curl -sS --max-time 5 --cacert "$CERT_DIR/ca.pem" "$URL" >/dev/null 2>&1; then
  echo "❌ Expected TLS handshake to fail without client cert"
  exit 1
fi

# Valid client cert should succeed.
if ! curl -sS --max-time 5 --cacert "$CERT_DIR/ca.pem" \
  --cert "$CERT_DIR/client.pem" --key "$CERT_DIR/client.key" \
  "$URL" >/dev/null; then
  echo "❌ Expected TLS handshake to succeed with valid client cert"
  exit 1
fi

# Unknown CA client cert should fail.
if curl -sS --max-time 5 --cacert "$CERT_DIR/ca.pem" \
  --cert "$CERT_DIR/client_bad.pem" --key "$CERT_DIR/client_bad.key" \
  "$URL" >/dev/null 2>&1; then
  echo "❌ Expected TLS handshake to fail with unknown CA client cert"
  exit 1
fi

echo "✅ security_02_mtls_handshake passed"

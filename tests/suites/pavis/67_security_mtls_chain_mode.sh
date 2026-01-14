#!/bin/bash
set -e

# Case: security_07_mtls_chain_mode
# Category: Security & TLS
# Description: Verifies client cert chain_mode handling for outbound mTLS.

# SKIP: Pingora's rustls connector does not support per-peer CA certificates yet.
# See: https://github.com/cloudflare/pingora/blob/main/pingora-core/src/connectors/tls/rustls/mod.rs
# TODO: Re-enable when pingora implements per-peer CA support or when switching to OpenSSL backend
echo "⏭️ SKIPPED: Pingora rustls does not support per-peer CA certificates"
exit 0

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "security_07"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_UPSTREAM=$(get_free_port)

CERT_DIR="$TEST_TMP/certs"
mkdir -p "$CERT_DIR"

# CA
cat > "$CERT_DIR/ca.cnf" <<EOF
[req]
distinguished_name = req_distinguished_name
x509_extensions = v3_ca
prompt = no
[req_distinguished_name]
CN = mtls-ca
[v3_ca]
basicConstraints = critical,CA:TRUE
keyUsage = critical, digitalSignature, cRLSign, keyCertSign
EOF

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/ca.key" \
  -out "$CERT_DIR/ca.pem" \
  -days 365 -config "$CERT_DIR/ca.cnf" >/dev/null 2>&1

# Server & Client
generate_signed_cert "server" "server" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "localhost"
generate_signed_cert "client" "client" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "pavis-client"

# Client bundle with embedded chain (leaf + CA)
cat "$CERT_DIR/client.pem" "$CERT_DIR/ca.pem" > "$CERT_DIR/client_bundle.pem"

openssl s_server -accept "$PORT_UPSTREAM" \
  -cert "$CERT_DIR/server.pem" \
  -key "$CERT_DIR/server.key" \
  -CAfile "$CERT_DIR/ca.pem" \
  -Verify 1 \
  -verify_return_error \
  -www > "$TEST_TMP/logs/mtls_chain_upstream.log" 2>&1 &
record_pid $! "mtls_chain_upstream"

if ! wait_for_port "$PORT_UPSTREAM" 5; then
  echo "❌ Upstream did not open port $PORT_UPSTREAM"
  exit 1
fi

cat <<EOF > "$TEST_TMP/config_embedded.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    tls:
      enabled: true
      verify_cert: true
      verify_hostname: true
      sni_mode: name
      sni: "localhost"
      ca_bundle_path: "$CERT_DIR/ca.pem"
      cert:
        cert_path: "$CERT_DIR/client_bundle.pem"
        key_path: "$CERT_DIR/client.key"
        chain_mode: embedded
    endpoints:
      - address: "127.0.0.1"
        port: $PORT_UPSTREAM
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        destinations:
          - upstream: "backend"
            weight: 1
EOF

gen_pvs "$TEST_TMP/config_embedded.yaml" "$TEST_TMP/config_embedded.pvs"
run_pavis "$TEST_TMP/config_embedded.pvs" ""
wait_for_url "http://127.0.0.1:$PORT_PAVIS/" 5
assert_status "http://127.0.0.1:$PORT_PAVIS/" "200"
stop_sut "pavis"

cat <<EOF > "$TEST_TMP/config_default.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    tls:
      enabled: true
      verify_cert: true
      verify_hostname: true
      sni_mode: name
      sni: "localhost"
      ca_bundle_path: "$CERT_DIR/ca.pem"
      cert:
        cert_path: "$CERT_DIR/client_bundle.pem"
        key_path: "$CERT_DIR/client.key"
    endpoints:
      - address: "127.0.0.1"
        port: $PORT_UPSTREAM
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        destinations:
          - upstream: "backend"
            weight: 1
EOF

gen_pvs "$TEST_TMP/config_default.yaml" "$TEST_TMP/config_default.pvs"

"$PAVIS_BIN" --config "$TEST_TMP/config_default.pvs" >"$TEST_TMP/logs/pavis_chain_fail.log" 2>&1 &
PAVIS_FAIL_PID=$!
sleep 1
if kill -0 "$PAVIS_FAIL_PID" 2>/dev/null; then
  echo "❌ Expected Pavis to fail when client cert bundle has multiple certs without chain_mode=embedded"
  kill "$PAVIS_FAIL_PID" 2>/dev/null || true
  wait "$PAVIS_FAIL_PID" 2>/dev/null || true
  exit 1
fi

if ! grep -q "client cert must contain exactly one certificate" "$TEST_TMP/logs/pavis_chain_fail.log"; then
  echo "❌ Expected client cert bundle error in logs"
  exit 1
fi

echo "✅ security_07_mtls_chain_mode passed"

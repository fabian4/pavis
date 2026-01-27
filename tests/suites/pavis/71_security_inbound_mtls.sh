#!/bin/bash
set -e

# Case: 71_security_inbound_mtls
# Category: Security & TLS
# Invariants: Inbound TLS termination + mTLS enforcement

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "71_security_inbound_mtls"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
CERT_DIR="$TEST_TMP/certs"
mkdir -p "$CERT_DIR"

# Root CA
cat > "$CERT_DIR/ca.cnf" <<EOF
[req]
distinguished_name = req_distinguished_name
x509_extensions = v3_ca
prompt = no
[req_distinguished_name]
CN = Pavis Test CA
[v3_ca]
basicConstraints = critical,CA:TRUE
keyUsage = critical, digitalSignature, cRLSign, keyCertSign
EOF

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/ca.key" \
  -out "$CERT_DIR/ca.pem" \
  -days 365 -config "$CERT_DIR/ca.cnf" >/dev/null 2>&1

# Server & Trusted Client
generate_signed_cert "server" "server" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "localhost"
generate_signed_cert "client" "client" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "trusted-client"

# Unknown CA
cat > "$CERT_DIR/ca_unknown.cnf" <<EOF
[req]
distinguished_name = req_distinguished_name
x509_extensions = v3_ca
prompt = no
[req_distinguished_name]
CN = Unknown CA
[v3_ca]
basicConstraints = critical,CA:TRUE
keyUsage = critical, digitalSignature, cRLSign, keyCertSign
EOF

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/ca_unknown.key" \
  -out "$CERT_DIR/ca_unknown.pem" \
  -days 365 -config "$CERT_DIR/ca_unknown.cnf" >/dev/null 2>&1

# Bad Client (signed by Unknown CA)
generate_signed_cert "client_bad" "client" "$CERT_DIR" "$CERT_DIR/ca_unknown.pem" "$CERT_DIR/ca_unknown.key" "bad-client"

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

make_config() {
    local yaml="$1"
    local mode="$2"
    
    echo "listeners:" > "$yaml"
    echo "  - name: \"https\"" >> "$yaml"
    echo "    address: \"127.0.0.1:$PORT_PAVIS\"" >> "$yaml"
    echo "    tls:" >> "$yaml"
    echo "      cert_path: \"$CERT_DIR/server.pem\"" >> "$yaml"
    echo "      key_path: \"$CERT_DIR/server.key\"" >> "$yaml"
    if [ -n "$mode" ]; then
        echo "      client_auth: !$mode" >> "$yaml"
        echo "        ca_path: \"$CERT_DIR/ca.pem\"" >> "$yaml"
    fi
    cat <<-EOF >> "$yaml"
upstreams:
  - name: "backend"
    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend", weight: 1 }]
EOF
}

make_config "$TEST_TMP/config_required.yaml" "required"
gen_pvs "$TEST_TMP/config_required.yaml" "$TEST_TMP/config_required.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_required.pvs"
cp "$TEST_TMP/config_required.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"

HTTPS_URL="https://127.0.0.1:$PORT_PAVIS/echo"
CURL_BASE=(curl -sS --connect-timeout 1 --max-time 3 --cacert "$CERT_DIR/ca.pem")

# Use explicit helpers to verify mTLS enforcement outcomes.
curl_expect_success() {
    if ! curl "${CURL_BASE[@]}" "$@" "$HTTPS_URL" >/dev/null 2>&1; then
        echo "❌ Expected curl success for $HTTPS_URL"
        exit 1
    fi
}

curl_expect_failure() {
    if curl "${CURL_BASE[@]}" "$@" "$HTTPS_URL" >/dev/null 2>&1; then
        echo "❌ Expected curl failure for $HTTPS_URL"
        exit 1
    fi
}

# Wait for TLS listener with client cert.
wait_for_url "$HTTPS_URL" 10 --cacert "$CERT_DIR/ca.pem" --cert "$CERT_DIR/client.pem" --key "$CERT_DIR/client.key"

# Require client certificate: no cert fails, valid cert succeeds.
curl_expect_failure
curl_expect_success --cert "$CERT_DIR/client.pem" --key "$CERT_DIR/client.key"

# Step 3: Unknown CA client should fail
curl_expect_failure --cert "$CERT_DIR/client_bad.pem" --key "$CERT_DIR/client_bad.key"

echo "✅ security_03_inbound_mtls passed"

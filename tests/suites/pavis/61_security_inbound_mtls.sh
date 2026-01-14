#!/bin/bash
set -e

# Case: security_03_inbound_mtls
# Category: Security & TLS
# Invariants: Inbound TLS termination + mTLS enforcement

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "security_03"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
CERT_DIR="$TEST_TMP/certs"
mkdir -p "$CERT_DIR"

# Root CA
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/ca.key" \
  -out "$CERT_DIR/ca.pem" \
  -subj "/CN=Pavis Test CA" -days 365 >/dev/null 2>&1

# Server certificate
openssl req -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/server.key" \
  -out "$CERT_DIR/server.csr" \
  -subj "/CN=localhost" >/dev/null 2>&1
openssl x509 -req -in "$CERT_DIR/server.csr" \
  -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca.key" -CAcreateserial \
  -out "$CERT_DIR/server.pem" -days 365 >/dev/null 2>&1

# Trusted client
openssl req -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/client.key" \
  -out "$CERT_DIR/client.csr" \
  -subj "/CN=trusted-client" >/dev/null 2>&1
openssl x509 -req -in "$CERT_DIR/client.csr" \
  -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca.key" -CAcreateserial \
  -out "$CERT_DIR/client.pem" -days 365 >/dev/null 2>&1

# Unknown CA + client
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

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

make_config() {
    local yaml="$1"
    local client_auth_block="$2"
    # echo -e interprets \n
    echo "listeners:" > "$yaml"
    echo "  - name: \"https\"" >> "$yaml"
    echo "    address: \"127.0.0.1:$PORT_PAVIS\"" >> "$yaml"
    echo "    tls:" >> "$yaml"
    echo "      cert_path: \"$CERT_DIR/server.pem\"" >> "$yaml"
    echo "      key_path: \"$CERT_DIR/server.key\"" >> "$yaml"
    if [ -n "$client_auth_block" ]; then
        echo -e "$client_auth_block" >> "$yaml"
    fi
    cat <<-EOF >> "$yaml"
upstreams:
  - name: "backend"
    endpoints: [{ ip: "127.0.0.1", port: 8081 }]
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        destinations: [{ upstream: "backend", weight: 1 }]
EOF
}

make_config "$TEST_TMP/config_tls.yaml" ""
gen_pvs "$TEST_TMP/config_tls.yaml" "$TEST_TMP/config_tls.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_tls.pvs"
cp "$TEST_TMP/config_tls.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_port "$PORT_PAVIS" 10

HTTPS_URL="https://localhost:$PORT_PAVIS/echo"
CURL_BASE=(--silent --show-error --max-time 5 --cacert "$CERT_DIR/ca.pem" --resolve "localhost:$PORT_PAVIS:127.0.0.1")

curl_expect_success() {
    if ! curl "${CURL_BASE[@]}" "$@" "$HTTPS_URL" >/dev/null; then
        echo "❌ Expected HTTPS request to succeed: $*"
        exit 1
    fi
}

curl_expect_failure() {
    if curl "${CURL_BASE[@]}" "$@" "$HTTPS_URL" >/dev/null 2>&1; then
        echo "❌ Expected HTTPS request to fail: $*"
        exit 1
    fi
}

# Step 1: TLS termination without client cert
curl_expect_success

gen_pvs_with_client_auth() {
    local mode="$1"
    local yaml="$TEST_TMP/config_${mode}.yaml"
    local block="      client_auth: !$mode\n        ca_path: \"$CERT_DIR/ca.pem\""
    make_config "$yaml" "$block"
    gen_pvs "$yaml" "$TEST_TMP/config_${mode}.pvs"
    publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_${mode}.pvs"
}

# Step 2: Require client certificate (valid succeeds)
gen_pvs_with_client_auth required
for _ in $(seq 1 10); do
    if curl "${CURL_BASE[@]}" --cert "$CERT_DIR/client.pem" --key "$CERT_DIR/client.key" "$HTTPS_URL" >/dev/null 2>&1; then
        READY=1
        break
    fi
    sleep 0.5
done
if [ -z "$READY" ]; then
    echo "❌ Runtime did not reload mTLS config in time"
    exit 1
fi
curl_expect_failure
curl_expect_success --cert "$CERT_DIR/client.pem" --key "$CERT_DIR/client.key"

# Step 3: Unknown CA client should fail
curl_expect_failure --cert "$CERT_DIR/client_bad.pem" --key "$CERT_DIR/client_bad.key"

echo "✅ security_03_inbound_mtls passed"
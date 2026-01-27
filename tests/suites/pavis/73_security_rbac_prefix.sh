#!/bin/bash
set -e

# Case: 73_security_rbac_prefix
# Category: Security (RBAC)
# Invariants: SPIFFE prefix-based authorization

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "73_security_rbac_prefix"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
CERT_DIR="$TEST_TMP/certs"
mkdir -p "$CERT_DIR"

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

generate_signed_cert "server" "server" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "localhost"
SPIFFE_PREFIX="spiffe://cluster.local/ns/prod/"
SPIFFE_ALLOWED_ADMIN="${SPIFFE_PREFIX}sa/admin"
SPIFFE_ALLOWED_VIEWER="${SPIFFE_PREFIX}sa/viewer"
SPIFFE_DENIED="spiffe://cluster.local/ns/dev/sa/intruder"
generate_spiffe_client_cert "client_admin" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "$SPIFFE_ALLOWED_ADMIN"
generate_spiffe_client_cert "client_viewer" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "$SPIFFE_ALLOWED_VIEWER"
generate_spiffe_client_cert "client_denied" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "$SPIFFE_DENIED"

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

cat <<-EOF > "$TEST_TMP/rbac_prefix.yaml"
	listeners:
	  - name: "https"
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
	        port: ${UPSTREAM_HTTP_PORT_V1}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        principal: !prefix
	          prefix: "$SPIFFE_PREFIX"
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF

gen_pvs "$TEST_TMP/rbac_prefix.yaml" "$TEST_TMP/rbac_prefix.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/rbac_prefix.pvs"
cp "$TEST_TMP/rbac_prefix.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"

HTTPS_URL="https://127.0.0.1:$PORT_PAVIS/echo"
CURL_BASE=(curl -sS --connect-timeout 2 --max-time 5 --cacert "$CERT_DIR/ca.pem")

wait_for_url "$HTTPS_URL" 10 --cacert "$CERT_DIR/ca.pem" --cert "$CERT_DIR/client_admin.pem" --key "$CERT_DIR/client_admin.key"

curl_expect_status() {
    local expected="$1"
    shift
    local status
    if ! status=$("${CURL_BASE[@]}" -o /dev/null -w "%{http_code}" "$@" "$HTTPS_URL"); then
        echo "❌ curl failed for $HTTPS_URL with args: $*"
        exit 1
    fi
    if [ "$status" != "$expected" ]; then
        echo "❌ Expected HTTP $expected, got $status"
        exit 1
    fi
}

curl_expect_failure() {
    if "${CURL_BASE[@]}" -o /dev/null "$HTTPS_URL" >/dev/null 2>&1; then
        echo "❌ Expected TLS handshake failure for $HTTPS_URL"
        exit 1
    fi
}

curl_expect_status 200 --cert "$CERT_DIR/client_admin.pem" --key "$CERT_DIR/client_admin.key"
curl_expect_status 200 --cert "$CERT_DIR/client_viewer.pem" --key "$CERT_DIR/client_viewer.key"
curl_expect_status 403 --cert "$CERT_DIR/client_denied.pem" --key "$CERT_DIR/client_denied.key"
curl_expect_failure

echo "✅ 73_security_rbac_prefix passed"

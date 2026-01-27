#!/bin/bash
set -e

# Case: 72_security_rbac_spiffe
# Category: Security (RBAC)
# Invariants: SPIFFE identity exact match authorization

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "72_security_rbac_spiffe"
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

generate_signed_cert "server" "server" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "localhost"
SPIFFE_ALLOWED="spiffe://cluster.local/ns/default/sa/admin"
SPIFFE_DENIED="spiffe://cluster.local/ns/other/sa/rogue"
generate_spiffe_client_cert "client_allowed" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "$SPIFFE_ALLOWED"
generate_spiffe_client_cert "client_denied" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "$SPIFFE_DENIED"

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

cat <<-EOF > "$TEST_TMP/rbac_spiffe.yaml"
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
	        principal: !authenticated
	          spiffe: "$SPIFFE_ALLOWED"
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF

gen_pvs "$TEST_TMP/rbac_spiffe.yaml" "$TEST_TMP/rbac_spiffe.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/rbac_spiffe.pvs"
cp "$TEST_TMP/rbac_spiffe.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"

HTTPS_URL="https://127.0.0.1:$PORT_PAVIS/echo"
CURL_BASE=(curl -sS --connect-timeout 2 --max-time 5 --cacert "$CERT_DIR/ca.pem")

wait_for_url "$HTTPS_URL" 10 --cacert "$CERT_DIR/ca.pem" --cert "$CERT_DIR/client_allowed.pem" --key "$CERT_DIR/client_allowed.key"

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

curl_expect_status 200 --cert "$CERT_DIR/client_allowed.pem" --key "$CERT_DIR/client_allowed.key"
curl_expect_status 403 --cert "$CERT_DIR/client_denied.pem" --key "$CERT_DIR/client_denied.key"
curl_expect_failure

echo "✅ 72_security_rbac_spiffe passed"

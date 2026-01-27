#!/bin/bash
set -e

# Case: 74_security_mtls_outbound
# Category: Security & TLS
# Invariants: C (Atomic Switch)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "74_security_mtls_outbound"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
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

# Server & Client certs
generate_signed_cert "server" "server" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "localhost"
generate_signed_cert "client" "client" "$CERT_DIR" "$CERT_DIR/ca.pem" "$CERT_DIR/ca.key" "pavis-client"

openssl s_server -accept "$PORT_UPSTREAM" \
    -cert "$CERT_DIR/server.pem" \
    -key "$CERT_DIR/server.key" \
    -CAfile "$CERT_DIR/ca.pem" \
    -Verify 1 \
    -verify_return_error \
    -www > "$TEST_TMP/logs/mtls_upstream.log" 2>&1 &
record_pid $! "mtls_upstream"

if ! wait_for_port "$PORT_UPSTREAM" 5; then
    echo "❌ Upstream did not open port $PORT_UPSTREAM"
    exit 1
fi

if openssl s_client -connect "127.0.0.1:$PORT_UPSTREAM" -quiet </dev/null >/dev/null 2>&1; then
    echo "❌ Expected upstream to require client cert"
    exit 1
fi

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

cat <<-EOF > "$TEST_TMP/config.yaml"
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
	        cert_path: "$CERT_DIR/client.pem"
	        key_path: "$CERT_DIR/client.key"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: $PORT_UPSTREAM
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
cp "$TEST_TMP/config.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

assert_status "http://127.0.0.1:$PORT_PAVIS/" "200"

echo "✅ security_05_mtls_outbound passed"

#!/bin/bash
set -e

# Case: security_05_mtls_outbound
# Category: Security & TLS
# Invariants: C (Atomic Switch)

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "security_05"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_UPSTREAM=$(get_free_port)

CERT_DIR="$TEST_TMP/certs"
mkdir -p "$CERT_DIR"

# CA
openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout "$CERT_DIR/ca.key" \
    -out "$CERT_DIR/ca.pem" \
    -subj "/CN=mtls-ca" -days 365 >/dev/null 2>&1

# Server cert
openssl req -newkey rsa:2048 -nodes \
    -keyout "$CERT_DIR/server.key" \
    -out "$CERT_DIR/server.csr" \
    -subj "/CN=localhost" >/dev/null 2>&1
openssl x509 -req -in "$CERT_DIR/server.csr" \
    -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca.key" -CAcreateserial \
    -out "$CERT_DIR/server.pem" -days 365 >/dev/null 2>&1

# Client cert
openssl req -newkey rsa:2048 -nodes \
    -keyout "$CERT_DIR/client.key" \
    -out "$CERT_DIR/client.csr" \
    -subj "/CN=pavis-client" >/dev/null 2>&1
openssl x509 -req -in "$CERT_DIR/client.csr" \
    -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca.key" -CAcreateserial \
    -out "$CERT_DIR/client.pem" -days 365 >/dev/null 2>&1

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
	      - matcher: !prefix { path: "/" }
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

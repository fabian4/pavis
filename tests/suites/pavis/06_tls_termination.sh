#!/bin/bash
set -e

# Case 06: TLS Termination
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_06"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

openssl req -x509 -newkey rsa:2048 -nodes -keyout "$TEST_TMP/key.pem" -out "$TEST_TMP/cert.pem" -subj "/CN=localhost" -days 1 2>/dev/null

cat <<-EOF > "$TEST_TMP/config.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	    tls:
	      cert_path: "$TEST_TMP/cert.pem"
	      key_path: "$TEST_TMP/key.pem"
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
wait_for_port "$PORT_PAVIS" 5

RESP=$(curl -k -s "https://127.0.0.1:$PORT_PAVIS/")
if [[ "$RESP" != *"backend-v1"* ]]; then echo "❌ Expected 'backend-v1', got '$RESP'"; exit 1; fi

echo "✅ Case 06_tls_termination passed"

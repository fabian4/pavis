#!/bin/bash
set -e

# Case 09: TLS Propagation
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "integrated_09"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)
PORT_PAVIS_TLS=$(get_free_port)

openssl req -x509 -newkey rsa:2048 -nodes -keyout "$TEST_TMP/key.pem" -out "$TEST_TMP/cert.pem" -subj "/CN=localhost" -days 1 2>/dev/null

mkdir -p "$TEST_TMP/storage"
cat <<-EOF > "$TEST_TMP/relay.yaml"
	identity:
	  name: "integrated-09"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  root_dir: "$TEST_TMP/storage"
	artifact:
	  lkg_path: "$TEST_TMP/storage/lkg.pvs"
	pipeline:
	  ingest:
	    source:
	      kind: file
	      path: "$TEST_TMP/ingest.yaml"
EOF

cat <<-EOF > "$TEST_TMP/ingest.yaml"
	listeners:
	  - name: "tls"
	    address: "127.0.0.1:$PORT_PAVIS_TLS"
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

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_pvs "$TEST_TMP/ingest.yaml" "$TEST_TMP/boot.pvs"
run_pavis "$TEST_TMP/boot.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_port "$PORT_PAVIS_TLS" 5

RESP=$(curl -k -s "https://127.0.0.1:$PORT_PAVIS_TLS/")
if [[ "$RESP" != *"backend-v1"* ]]; then echo "❌ HTTPS propagation failed"; exit 1; fi

echo "✅ Case 09_tls_propagation passed"

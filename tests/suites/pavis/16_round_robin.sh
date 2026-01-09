#!/bin/bash
set -e

# Case 16: Round Robin
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_16"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

cat <<-EOF > "$TEST_TMP/config.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "cluster"
	    balancer: round-robin
	    endpoints:
	      - address: "127.0.0.1"
	        port: 8081
	      - address: "127.0.0.1"
	        port: 8082
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix
	          path: "/"
	        destinations:
	          - upstream: "cluster"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

run_pavis "$TEST_TMP/config.pvs" ""
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

c1=0; c2=0
for i in {1..20}; do
    RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS")
    if [[ "$RESP" == *"backend-v1"* ]]; then c1=$((c1+1)); else c2=$((c2+1)); fi
done

if [ "$c1" -lt 5 ] || [ "$c2" -lt 5 ]; then echo "❌ Uneven distribution: $c1 vs $c2"; exit 1; fi

echo "✅ Case 16_round_robin passed"

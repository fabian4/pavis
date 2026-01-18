#!/bin/bash
set -e

# Case: traffic_02_weighted_shift
# Category: Traffic Behavior Under Reload
# Invariants: A (No-Drop)
# Intent: deterministic state carry-over elimination (100/0 flips), not probabilistic weight correctness.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "traffic_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# V1: 100% v1 (Explicit single destination)
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	  - name: "v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Assert V1 (100% backend-v1)

for _ in {1..20}; do

    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")

    if [[ "$response" != *"backend-v1"* ]]; then

        echo "❌ Expected only backend-v1 in V1"

        exit 1

    fi

done



# V2: 0% v1, 100% v2 (Weight flip)

cat <<-EOF > "$TEST_TMP/config_v2.yaml"

	listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]

	upstreams:

	  - name: "v1"

	    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]

	  - name: "v2"

	    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]

	routes:

	  - host: "*"

	    paths:

	      - matcher: !prefix { path: "/" }

	        destinations: [{ upstream: "v2", weight: 1 }]

EOF

gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"



# Wait for switch (poll for v2 presence)

MAX_RETRIES=20

SWITCHED=0

for _ in $(seq 1 $MAX_RETRIES); do

    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")

    if [[ "$response" == *"backend-v2"* ]]; then

        SWITCHED=1

        break

    fi

    sleep 0.5

done



if [ "$SWITCHED" -eq 0 ]; then

    echo "❌ Traffic did not shift to backend-v2"

    exit 1

fi



# Assert V2 (100% backend-v2)

for _ in {1..20}; do

    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")

    if [[ "$response" != *"backend-v2"* ]]; then

        echo "❌ Expected only backend-v2 in V2 (Weight flip failed)"

        exit 1

    fi

done



# V3: Deterministic flip back to 100% V1



cat <<-EOF > "$TEST_TMP/config_v3.yaml"



	listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]



	upstreams:



	  - name: "v1"



	    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }]



	  - name: "v2"



	    endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }]



	routes:



	  - host: "*"



	    paths:



	      - matcher: !prefix { path: "/" }



	        destinations: [{ upstream: "v1", weight: 1 }]



EOF







gen_pvs "$TEST_TMP/config_v3.yaml" "$TEST_TMP/config_v3.pvs"



publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v3.pvs"







# Wait for switch back (poll for v1 presence)



SWITCHED_BACK=0



for _ in $(seq 1 $MAX_RETRIES); do



    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")



    if [[ "$response" == *"backend-v1"* ]]; then



        SWITCHED_BACK=1



        break



    fi



    sleep 0.5



done







if [ "$SWITCHED_BACK" -eq 0 ]; then



    echo "❌ Traffic did not shift back to backend-v1"



    exit 1



fi







# Assert V3 (100% backend-v1)



for _ in {1..20}; do



    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")



    if [[ "$response" != *"backend-v1"* ]]; then



        echo "❌ Expected only backend-v1 in V3 (Deterministic weight flip failed)"



        exit 1



    fi



done







echo "✅ traffic_02_weighted_shift passed"

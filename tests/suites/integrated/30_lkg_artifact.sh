#!/bin/bash
set -e

# Case: lkg_01_relay_bad_artifact
# Category: Failure & LKG
# Invariants: I3 (Artifact Opaqueness), I4 (System LKG)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "lkg_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	artifact:
	  lkg_path: "$TEST_TMP/lkg.pvs"
EOF
run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

cat <<-EOF > "$TEST_TMP/config.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8081
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/config.pvs" > /dev/null

cp "$TEST_TMP/config.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Assert V1
assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"

# Publish Corrupt Data
# We attempt to publish invalid data. Relay might reject (422) or accept.
# If relay rejects, fine. If relay accepts, Runtime must reject.
# The goal is "System LKG".
echo "CORRUPT" > "$TEST_TMP/corrupt.pvs"
RESP=$(curl -s -o /dev/null -w "%{http_code}" -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$TEST_TMP/corrupt.pvs")

echo "Publish response: $RESP"

sleep 2

# Assert Traffic Continues

assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"



# 5. Recovery Proof: Publish Valid V3

cat <<-EOF > "$TEST_TMP/config_v3.yaml"

	listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]

	upstreams: [{ name: "backend-v3", endpoints: [{ ip: "127.0.0.1", port: 8082 }] }]

	routes: [{ host: "*", paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: "backend-v3", weight: 1 }] }] }]

EOF

gen_pvs "$TEST_TMP/config_v3.yaml" "$TEST_TMP/config_v3.pvs"

curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" -H "x-pavis-version: 3" --data-binary "@$TEST_TMP/config_v3.pvs" > /dev/null



# 6. Assert Switch to V3

MAX_RETRIES=20

SWITCHED=0

for i in $(seq 1 $MAX_RETRIES); do

    if pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo" | grep -q "backend-v2"; then

        SWITCHED=1

        break

    fi

    sleep 0.5

done



if [ "$SWITCHED" -eq 0 ]; then

    echo "❌ Integrated recovery failed"

    exit 1

fi



echo "✅ 30_lkg_artifact passed"

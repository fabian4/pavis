#!/bin/bash
set -e

# Case 03: Concurrency
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "integrated_03"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)
P1=$(get_free_port); P2=$(get_free_port); P3=$(get_free_port)

mkdir -p "$TEST_TMP/storage"
cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: integrated-03 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline: { ingest: { source: { kind: file, path: "$TEST_TMP/ingest.yaml" } } }
EOF

cat <<EOF > "$TEST_TMP/ingest.yaml"
listeners: [{ name: default, address: "0.0.0.0:8080" }]
upstreams: [{ name: backend, endpoints: [{ ip: "127.0.0.1", port: 8081 }] }]
routes: [{ host: "*", paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: backend, weight: 1 }] }] }]
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_instance() {
    local p=$1
    local out=$2
    cat <<EOF > "$TEST_TMP/config_$p.yaml"
listeners: [{ name: default, address: "127.0.0.1:$p" }]
upstreams: [{ name: backend, endpoints: [{ ip: "127.0.0.1", port: 8081 }] }]
routes: [{ host: "*", paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: backend, weight: 1 }] }] }]
EOF
    gen_pvs "$TEST_TMP/config_$p.yaml" "$out"
}

gen_instance $P1 "$TEST_TMP/b1.pvs"
gen_instance $P2 "$TEST_TMP/b2.pvs"
gen_instance $P3 "$TEST_TMP/b3.pvs"

run_pavis "$TEST_TMP/b1.pvs" "http://127.0.0.1:$PORT_RELAY" "p1"
run_pavis "$TEST_TMP/b2.pvs" "http://127.0.0.1:$PORT_RELAY" "p2"
run_pavis "$TEST_TMP/b3.pvs" "http://127.0.0.1:$PORT_RELAY" "p3"

wait_for_url "http://127.0.0.1:$P1" 5
wait_for_url "http://127.0.0.1:$P2" 5
wait_for_url "http://127.0.0.1:$P3" 5

cat <<EOF > "$TEST_TMP/ingest.yaml"
listeners: []
upstreams: [{ name: backend, endpoints: [{ ip: "127.0.0.1", port: 8082 }] }]
routes: [{ host: "*", paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: backend, weight: 1 }] }] }]
EOF

echo "Waiting for convergence..."
for p in $P1 $P2 $P3; do
    SUCCESS=0
    for i in {1..20}; do
        if [[ "$(curl -s "http://127.0.0.1:$p")" == *"backend-v2"* ]]; then SUCCESS=1; break; fi
        sleep 1
    done
    if [ $SUCCESS -eq 0 ]; then echo "❌ $p failed to converge"; exit 1; fi
done

echo "✅ Case 03_concurrency passed"

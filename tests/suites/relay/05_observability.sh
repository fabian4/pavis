#!/bin/bash
set -e

# Case 05: Observability
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_05"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: relay-05 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline: { ingest: { source: { kind: file, path: "$TEST_TMP/ingest.yaml" } } }
EOF
touch "$TEST_TMP/ingest.yaml"

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

METRICS_START=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/metrics")
FAIL_START=$(echo "$METRICS_START" | grep "^pavis_relay_publish_fail_total" | awk '{print $2}' || echo 0)

curl -s -o /dev/null -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" -H "X-Pavis-Version: 999" --data "garbage"

METRICS_END=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/metrics")
FAIL_END=$(echo "$METRICS_END" | grep "^pavis_relay_publish_fail_total" | awk '{print $2}' || echo 0)

if [ "$FAIL_END" -le "$FAIL_START" ]; then echo "❌ Metric not incremented"; exit 1; fi

echo "✅ Case 05_observability passed"

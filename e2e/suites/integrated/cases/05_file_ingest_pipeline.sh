#!/bin/bash
set -e

# e2e/suites/integrated/cases/05_file_ingest_pipeline.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8303
PORT_PAVIS=8080
PORT_BACKEND_A=8081
PORT_BACKEND_B=8082

CASE_TMP=$(ensure_tmp_dir "integrated_05")

cleanup() {
    stop_pid "$CASE_TMP/backend_a.pid"
    stop_pid "$CASE_TMP/backend_b.pid"
    stop_pid "$CASE_TMP/pavis.pid"
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Start backends
start_backend $PORT_BACKEND_A "A" "$CASE_TMP/backend_a.pid"
start_backend $PORT_BACKEND_B "B" "$CASE_TMP/backend_b.pid"

# 2. Setup relay with file ingest
mkdir -p "$CASE_TMP/relay_storage"
INGEST_FILE="$CASE_TMP/input.yaml"
echo "{}" > "$INGEST_FILE"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-pipeline
http:
  bind: "127.0.0.1:$PORT_RELAY"
storage:
  root_dir: "$CASE_TMP/relay_storage"
artifact:
  lkg_path: "$CASE_TMP/relay_storage/lkg.pvs"
pipeline:
  ingest:
    source:
      kind: file
      path: "$INGEST_FILE"
    debounce_ms: 300
EOFCONFIG

RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 3. Start Pavis connected to relay
"$PAVIS_BIN" --relay-url "http://127.0.0.1:$PORT_RELAY" > "$CASE_TMP/pavis.log" 2>&1 &
echo $! > "$CASE_TMP/pavis.pid"
sleep 2

# 4. Write v1 config to ingest file (route to A)
cat <<EOFYAML > "$INGEST_FILE"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend-a"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_A
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-a"
            weight: 1
EOFYAML

# Wait for debounce + processing
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 10

# 5. Verify traffic goes to A
assert_body "http://127.0.0.1:$PORT_PAVIS/" "A"

# 6. Update ingest file to v2 (route to B)
cat <<EOFYAML > "$INGEST_FILE"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend-b"
    endpoints:
      - ip: "127.0.0.1"
        port: $PORT_BACKEND_B
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend-b"
            weight: 1
EOFYAML

# Wait for relay to process and runtime to apply
sleep 2

# 7. Verify traffic now goes to B
for attempt in {1..10}; do
    RESP=$(curl -s "http://127.0.0.1:$PORT_PAVIS/" 2>/dev/null || echo "ERROR")
    if [ "$RESP" == "B" ]; then
        echo "✅ Case 05_file_ingest_pipeline passed"
        exit 0
    fi
    sleep 1
done

echo "ERROR: Traffic should have switched to B"
exit 1

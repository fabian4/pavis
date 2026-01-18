#!/bin/bash
set -e

# Case: reload_keepalive_atomic
# Category: Reload Semantics
# Invariants: A (No-Drop), C (Atomic Switch)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "reload_keepalive_atomic"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	  - name: "backend-v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        response_headers:
	          set_headers: [["X-Pavis-Version", "v1"]]
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	  - name: "backend-v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        response_headers:
	          set_headers: [["X-Pavis-Version", "v2"]]
	        destinations:
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

wait_for_version() {
    local expected_version="$1"
    local expected_instance="$2"
    local timeout="${3:-10}"
    local start_time
    start_time=$(date +%s)
    while true; do
        headers="$TEST_TMP/wait.headers"
        body="$TEST_TMP/wait.body"
        if curl -sS -D "$headers" -o "$body" "http://127.0.0.1:$PORT_PAVIS/echo"; then
            got_version=$(awk 'tolower($1)=="x-pavis-version:" {print $2}' "$headers" | tr -d '\r')
            got_instance=$(python3 -c "import sys, json; print(json.load(sys.stdin).get('instance_id',''))" < "$body")
            if [ "$got_version" = "$expected_version" ] && [ "$got_instance" = "$expected_instance" ]; then
                return 0
            fi
        fi
        local now
        now=$(date +%s)
        if [ $((now - start_time)) -ge "$timeout" ]; then
            echo "Timeout waiting for ${expected_version}/${expected_instance}"
            return 1
        fi
        sleep 0.2
    done
}

KEEPALIVE_OUT="$TEST_TMP/keepalive_results.txt"
KEEPALIVE_GO="$TEST_TMP/reload_go"

python3 - "$PORT_PAVIS" "$KEEPALIVE_OUT" "$KEEPALIVE_GO" <<'PY' &
import sys
import json
import time
import http.client
import os

port = int(sys.argv[1])
out_path = sys.argv[2]
go_path = sys.argv[3]

def write_line(line):
    with open(out_path, "a", encoding="utf-8") as f:
        f.write(line + "\n")

conn = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
try:
    conn.request("GET", "/echo", headers={"Connection": "keep-alive"})
    resp = conn.getresponse()
    body = resp.read()
    version = resp.getheader("X-Pavis-Version")
    instance = json.loads(body).get("instance_id", "")
    write_line(f"phase=before version={version} instance={instance}")
except Exception as exc:
    write_line(f"phase=before error={exc}")
    sys.exit(1)

start = time.time()
while not os.path.exists(go_path):
    if time.time() - start > 10:
        write_line("phase=wait error=timeout")
        sys.exit(1)
    time.sleep(0.1)

for i in range(10):
    try:
        conn.request("GET", "/echo", headers={"Connection": "keep-alive"})
        resp = conn.getresponse()
        body = resp.read()
        version = resp.getheader("X-Pavis-Version")
        instance = json.loads(body).get("instance_id", "")
        write_line(f"phase=after version={version} instance={instance}")
    except Exception as exc:
        write_line(f"phase=after error={exc}")
        sys.exit(1)
PY
KEEPALIVE_PID=$!

sleep 0.5
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"
wait_for_version "v2" "backend-v2" 15
touch "$KEEPALIVE_GO"

wait $KEEPALIVE_PID

if grep -q "error=" "$KEEPALIVE_OUT"; then
    cat "$KEEPALIVE_OUT"
    echo "❌ Keep-alive flow failed during reload"
    exit 1
fi

before_line=$(grep "phase=before" "$KEEPALIVE_OUT" | tail -n 1)
after_lines=$(grep "phase=after" "$KEEPALIVE_OUT")
if [ -z "$after_lines" ]; then
    echo "❌ No post-reload samples captured"
    cat "$KEEPALIVE_OUT"
    exit 1
fi

before_version=$(echo "$before_line" | awk '{print $2}' | cut -d= -f2)
before_instance=$(echo "$before_line" | awk '{print $3}' | cut -d= -f2)
if [ "$before_version" != "v1" ] || [ "$before_instance" != "backend-v1" ]; then
    echo "❌ Pre-reload mismatch: $before_line"
    exit 1
fi

while read -r line; do
    version=$(echo "$line" | awk '{print $2}' | cut -d= -f2)
    instance=$(echo "$line" | awk '{print $3}' | cut -d= -f2)
    if [ "$version" != "v2" ] || [ "$instance" != "backend-v2" ]; then
        echo "❌ Post-reload mismatch: $line"
        exit 1
    fi
done <<< "$after_lines"

echo "✅ reload_keepalive_atomic passed"

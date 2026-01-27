#!/bin/bash
set -e

# Case: 22_reload_keepalive_atomic
# Category: Reload Semantics
# Invariants: A (No-Drop), C (Atomic Switch)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "22_reload_keepalive_atomic"
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
	      - matcher:
	          path: !prefix { path: "/" }
	        response_headers:
	          set_headers: [["X-Backend-Version", "v1"]]
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
	      - matcher:
	          path: !prefix { path: "/" }
	        response_headers:
	          set_headers: [["X-Backend-Version", "v2"]]
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
            got_version=$(awk 'tolower($1)=="x-backend-version:" {print $2}' "$headers" | tr -d '\r')
            got_instance=$(json_get_string "instance_id" < "$body")
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
KEEPALIVE_READY="$TEST_TMP/keepalive_ready"

keepalive_client() {
    local port="$1"
    local out_path="$2"
    local go_path="$3"

    exec 3<>"/dev/tcp/127.0.0.1/${port}" || {
        echo "phase=before error=connect" >> "$out_path"
        return 1
    }

    keepalive_request() {
        local phase="$1"
        local status_line
        local header
        local version=""
        local content_length=""
        local connection_header=""
        local body
        local instance

        printf 'GET /echo HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: keep-alive\r\n\r\n' >&3
        if ! IFS= read -r status_line <&3; then
            echo "phase=${phase} error=read_status" >> "$out_path"
            return 1
        fi
        : "$status_line"
        while IFS= read -r header <&3; do
            [ "$header" = $'\r' ] && break
            case "$header" in
                # Keepalive samples must track the reload marker header.
                [Xx]-[Bb][Aa][Cc][Kk][Ee][Nn][Dd]-[Vv]ersion:*)
                    version=$(echo "$header" | awk '{print $2}' | tr -d '\r')
                    ;;
                [Cc][Oo][Nn][Tt][Ee][Nn][Tt]-[Ll]ength:*)
                    content_length=$(echo "$header" | awk '{print $2}' | tr -d '\r')
                    ;;
                [Cc][Oo][Nn][Nn][Ee][Cc][Tt][Ii][Oo][Nn]:*)
                    connection_header=$(echo "$header" | awk '{print $2}' | tr -d '\r')
                    ;;
            esac
        done
        if [ -n "$connection_header" ] && [ "$connection_header" = "close" ]; then
            echo "phase=${phase} error=connection_closed" >> "$out_path"
            return 1
        fi
        if [ -z "$content_length" ]; then
            echo "phase=${phase} error=missing_length" >> "$out_path"
            return 1
        fi
        body=$(dd bs=1 count="$content_length" <&3 2>/dev/null)
        instance=$(printf '%s' "$body" | json_get_string "instance_id")
        if [ -z "$instance" ]; then
            echo "phase=${phase} error=missing_instance" >> "$out_path"
            return 1
        fi
        echo "phase=${phase} version=${version} instance=${instance}" >> "$out_path"
    }

    keepalive_request "before" || return 1
    touch "$KEEPALIVE_READY"

    local start
    start=$(date +%s)
    while [ ! -f "$go_path" ]; do
        if [ $(( $(date +%s) - start )) -gt 10 ]; then
            echo "phase=wait error=timeout" >> "$out_path"
            return 1
        fi
        sleep 0.1
    done

    for _ in $(seq 1 10); do
        keepalive_request "after" || return 1
    done
}

keepalive_client "$PORT_PAVIS" "$KEEPALIVE_OUT" "$KEEPALIVE_GO" &
KEEPALIVE_PID=$!

for _ in $(seq 1 50); do
    [ -f "$KEEPALIVE_READY" ] && break
    sleep 0.1
done
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

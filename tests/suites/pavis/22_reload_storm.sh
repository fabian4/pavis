#!/bin/bash
set -e

# Case: reload_storm
# Category: Reload Semantics
# Invariants: A (No-Drop), C (Atomic Switch)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "reload_storm"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

write_config() {
    local version="$1"
    local upstream="$2"
    cat <<-EOF
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
	          set_headers: [["X-Pavis-Version", "v${version}"]]
	        destinations:
	          - upstream: "${upstream}"
	            weight: 1
EOF
}

write_config 1 "backend-v1" > "$TEST_TMP/config_v1.yaml"
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

TOTAL_TRAFFIC=400
(
    for i in $(seq 1 $TOTAL_TRAFFIC); do
        headers="$TEST_TMP/traffic_${i}.headers"
        body="$TEST_TMP/traffic_${i}.body"
        if ! curl -sS -D "$headers" -o "$body" "http://127.0.0.1:$PORT_PAVIS/echo"; then
            echo "FAIL" > "$TEST_TMP/traffic_${i}.fail"
            continue
        fi
        version=$(awk 'tolower($1)=="x-pavis-version:" {print $2}' "$headers" | tr -d '\r')
        if [ -z "$version" ]; then
            echo "FAIL" > "$TEST_TMP/traffic_${i}.fail"
            continue
        fi
        if ! instance=$(json_get_string "instance_id" < "$body"); then
            echo "FAIL" > "$TEST_TMP/traffic_${i}.fail"
            continue
        fi
        echo "${version},${instance}" > "$TEST_TMP/traffic_${i}.info"
    done
) &
TRAFFIC_PID=$!

for v in $(seq 2 10); do
    if [ $((v % 2)) -eq 0 ]; then
        upstream="backend-v2"
    else
        upstream="backend-v1"
    fi
    write_config "$v" "$upstream" > "$TEST_TMP/config_v${v}.yaml"
    gen_pvs "$TEST_TMP/config_v${v}.yaml" "$TEST_TMP/config_v${v}.pvs"
    publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v${v}.pvs"

    expected_version="v${v}"
    expected_instance="$upstream"
    SWITCHED=0
    for _ in $(seq 1 20); do
        headers="$TEST_TMP/switch_v${v}.headers"
        body="$TEST_TMP/switch_v${v}.body"
        if curl -sS -D "$headers" -o "$body" "http://127.0.0.1:$PORT_PAVIS/echo"; then
            got_version=$(awk 'tolower($1)=="x-pavis-version:" {print $2}' "$headers" | tr -d '\r')
            got_instance=$(json_get_string "instance_id" < "$body")
            if [ "$got_version" = "$expected_version" ] && [ "$got_instance" = "$expected_instance" ]; then
                SWITCHED=1
                break
            fi
        fi
        sleep 0.2
    done

    if [ "$SWITCHED" -eq 0 ]; then
        echo "❌ Reload did not apply version ${expected_version}"
        exit 1
    fi

    for i in $(seq 1 10); do
        headers="$TEST_TMP/post_v${v}_${i}.headers"
        body="$TEST_TMP/post_v${v}_${i}.body"
        if ! curl -sS -D "$headers" -o "$body" "http://127.0.0.1:$PORT_PAVIS/echo"; then
            echo "❌ Post-switch request failed for v${v}"
            exit 1
        fi
        got_version=$(awk 'tolower($1)=="x-pavis-version:" {print $2}' "$headers" | tr -d '\r')
        if ! got_instance=$(json_get_string "instance_id" < "$body"); then
            echo "❌ Failed to parse post-switch body for v${v}"
            exit 1
        fi
        if [ "$got_version" != "$expected_version" ] || [ "$got_instance" != "$expected_instance" ]; then
            echo "❌ Post-switch request mismatch for v${v}: ${got_version}/${got_instance}"
            exit 1
        fi
    done
done

wait $TRAFFIC_PID
TRAFFIC_STATUS=$?
if [ "$TRAFFIC_STATUS" -ne 0 ]; then
    echo "❌ Traffic sampler exited with status $TRAFFIC_STATUS"
    exit 1
fi

shopt -s nullglob
fail_files=("$TEST_TMP"/traffic_*.fail)
shopt -u nullglob
FAIL_COUNT=${#fail_files[@]}
if [ "$FAIL_COUNT" -gt 0 ]; then
    echo "❌ Invariant A violated: $FAIL_COUNT requests failed during reload storm"
    exit 1
fi

MAX_SEEN=0
for i in $(seq 1 $TOTAL_TRAFFIC); do
    info="$TEST_TMP/traffic_${i}.info"
    if [ ! -f "$info" ]; then
        continue
    fi
    entry=$(cat "$info")
    version=$(echo "$entry" | awk -F',' '{print $1}')
    instance=$(echo "$entry" | awk -F',' '{print $2}')
    if [ -z "$version" ]; then
        echo "❌ Missing X-Pavis-Version in traffic sample ${i}"
        exit 1
    fi
    version_num=$(echo "$version" | tr -d 'v')
    if [ $((version_num % 2)) -eq 0 ]; then
        expected_instance="backend-v2"
    else
        expected_instance="backend-v1"
    fi
    if [ "$instance" != "$expected_instance" ]; then
        echo "❌ Version/instance mismatch in sample ${i}: ${version}/${instance}"
        exit 1
    fi
    if [ "$version_num" -lt "$MAX_SEEN" ]; then
        echo "❌ Monotonicity violated: saw v${version_num} after v${MAX_SEEN}"
        exit 1
    fi
    if [ "$version_num" -gt "$MAX_SEEN" ]; then
        MAX_SEEN=$version_num
    fi
done

echo "✅ reload_storm passed"

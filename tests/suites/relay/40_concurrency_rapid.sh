#!/bin/bash
set -e

# Case: 40_concurrency_rapid
# Category: Concurrency
# Invariants: R5 (Concurrency Safety), R2 (Versioned)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "40_concurrency_rapid"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF_INNER > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	distribution:
	  long_poll:
	    enabled: true
EOF_INNER

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

for i in {1..50}; do
    gen_minimal_pvs "$TEST_TMP/payload-$i.pvs" "payload-$i"
done

(
    for i in {1..50}; do
        pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
            --data-binary "@$TEST_TMP/payload-$i.pvs" >/dev/null || echo "Pub $i failed"
    done
) &
PUB_PID=$!

ETAG=""
LAST_VER="0"
(
    for i in {1..100}; do
        if [ -n "$ETAG" ]; then
            if ! pavis_curl_headers "$TEST_TMP/resp_$i" -H "If-None-Match: $ETAG" \
                "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=1000"; then
                continue
            fi
        else
            if ! pavis_curl_headers "$TEST_TMP/resp_$i" \
                "http://127.0.0.1:$PORT_RELAY/v1/config"; then
                continue
            fi
        fi

        CODE=$(extract_status_code "$TEST_TMP/resp_$i")
        if [ "$CODE" = "200" ]; then
            V=$(header_value "$TEST_TMP/resp_$i" "x-config-version")
            if [ "$V" -lt "$LAST_VER" ]; then
                echo "FAIL: Version decreased: $V < $LAST_VER" > "$TEST_TMP/sub_fail"
                exit 1
            fi
            LAST_VER=$V
            ETAG=$(extract_etag "$TEST_TMP/resp_$i")
        fi

        if [ "$LAST_VER" = "50" ]; then
            echo "DONE" > "$TEST_TMP/sub_done"
            break
        fi

        if ! kill -0 $PUB_PID 2>/dev/null; then
            if [ -n "$ETAG" ]; then
                if ! pavis_curl_headers "$TEST_TMP/final_check" -H "If-None-Match: $ETAG" \
                    "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=1000"; then
                    continue
                fi
            else
                if ! pavis_curl_headers "$TEST_TMP/final_check" \
                    "http://127.0.0.1:$PORT_RELAY/v1/config"; then
                    continue
                fi
            fi
            CODE=$(extract_status_code "$TEST_TMP/final_check")
            if [ "$CODE" = "200" ]; then
                V=$(header_value "$TEST_TMP/final_check" "x-config-version")
                if [ "$V" -lt "$LAST_VER" ]; then
                    echo "FAIL: Version decreased in final check: $V < $LAST_VER" > "$TEST_TMP/sub_fail"
                    exit 1
                fi
                LAST_VER=$V
                ETAG=$(extract_etag "$TEST_TMP/final_check")
            fi
            if [ "$LAST_VER" = "50" ]; then
                echo "DONE" > "$TEST_TMP/sub_done"
                break
            fi
        fi
    done
) &
SUB_PID=$!

SUB_STATUS=0
if ! wait $PUB_PID; then
    echo "❌ Publisher loop failed"
    exit 1
fi
wait $SUB_PID || SUB_STATUS=$?

if [ -f "$TEST_TMP/sub_fail" ]; then
    cat "$TEST_TMP/sub_fail"
    exit 1
fi

if [ ! -f "$TEST_TMP/sub_done" ] && [ "$SUB_STATUS" -ne 0 ]; then
    echo "❌ Subscriber loop exited early"
    exit 1
fi

if [ ! -f "$TEST_TMP/sub_done" ]; then
    if [ -n "$ETAG" ]; then
        pavis_curl_headers "$TEST_TMP/final" -H "If-None-Match: $ETAG" \
            "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=1000"
    else
        pavis_curl_headers "$TEST_TMP/final" "http://127.0.0.1:$PORT_RELAY/v1/config"
    fi
    CODE=$(extract_status_code "$TEST_TMP/final")
    if [ "$CODE" = "200" ]; then
        V=$(header_value "$TEST_TMP/final" "x-config-version")
        if [ "$V" -lt "$LAST_VER" ]; then
            echo "❌ Final version decreased: $V < $LAST_VER"
            exit 1
        fi
        LAST_VER=$V
    fi
    if [ "$LAST_VER" != "50" ]; then
        echo "❌ Final state is $LAST_VER, expected 50"
        exit 1
    fi
fi

if ! pavis_curl_body -f "http://127.0.0.1:$PORT_RELAY/health" >/dev/null; then
    echo "❌ Relay died"
    exit 1
fi

echo "✅ concurrency_01_rapid_publish passed"

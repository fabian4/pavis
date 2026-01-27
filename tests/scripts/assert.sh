#!/bin/bash

# tests/scripts/assert.sh
# Assertions and Wait helpers.

assert_body() {
    local url="$1"
    local expected="$2"
    shift 2
    local actual
    actual=$(pavis_curl_body "$url" "$@")
    echo "DEBUG: assert_body got '$actual'"
    if [[ "$actual" != *"$expected"* ]]; then
        echo "❌ Assertion failed: Expected body to contain '$expected', got '$actual'"
        return 1
    fi
}

assert_status() {
    local url="$1"
    local expected="$2"
    shift 2
    local actual
    actual=$(pavis_curl_body -o /dev/null -w "%{http_code}" "$url" "$@")
    if [ "$actual" != "$expected" ]; then
        echo "❌ Assertion failed: Expected status $expected, got $actual"
        return 1
    fi
}

header_value() {
    local file="$1"
    local header_name="$2"
    # Case-insensitive grep for the header, using -a to treat as text even if binary body exists
    # We use sed to extract the value after the first colon and space, and trim \r
    grep -ai "^$header_name:" "$file" | head -n 1 | sed -E "s/^[^:]+:[[:space:]]*//i" | tr -d '\r' | tr -d '\n' | tr -d ' '
}

assert_status_eq() {
    local file="$1"
    local expected="$2"
    # First line of curl -i is "HTTP/1.1 200 OK"
    local actual
    actual=$(head -n 1 "$file" | awk '{print $2}')
    if [ "$actual" != "$expected" ]; then
        echo "❌ Assertion failed: Expected status $expected, got $actual"
        return 1
    fi
}

assert_header_eq() {
    local file="$1"
    local name="$2"
    local expected="$3"
    local actual
    actual=$(header_value "$file" "$name")
    if [ "$actual" != "$expected" ]; then
        echo "❌ Assertion failed: Expected header '$name' to be '$expected', got '$actual'"
        return 1
    fi
}

assert_json_has_key() {
    local key="$1"
    # Read JSON from stdin
    local body
    body=$(cat)
    if ! printf '%s' "$body" | grep -q "\"$key\"" ; then
        echo "❌ JSON assertion failed: Key '$key' missing in response"
        return 1
    fi
}

wait_for_url() {
    local url="$1"
    local timeout="${2:-30}"
    shift 2
    local extra_args=("$@")
    local start_time
    start_time=$(date +%s)

    while true;
 do
        if curl -s --connect-timeout 1 --max-time 2 -o /dev/null "${extra_args[@]}" "$url"; then
            return 0
        fi
        local current_time
        current_time=$(date +%s)
        if [ $((current_time - start_time)) -ge "$timeout" ]; then
            echo "Timeout waiting for $url"
            return 1
        fi
        sleep 0.5
    done
}

wait_for_port() {
    local port="$1"
    local timeout="${2:-10}"
    local start_time
    start_time=$(date +%s)

    while true;
 do
        if nc -z 127.0.0.1 "$port" 2>/dev/null;
 then
            return 0
        fi
        local current_time
        current_time=$(date +%s)
        if [ $((current_time - start_time)) -ge "$timeout" ]; then
            return 1
        fi
        sleep 0.2
    done
}

get_admin_version() {
    local admin_url="$1"
    pavis_curl_body "${admin_url}/stats" \
        | tr -d '\n' \
        | sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p'
}

wait_for_admin_version() {
    local admin_url="$1"
    local expected="$2"
    local timeout="${3:-10}"
    local start_time
    start_time=$(date +%s)

    while true;
 do
        local version
        version=$(get_admin_version "$admin_url")
        if [ "$version" = "$expected" ]; then
            return 0
        fi
        local current_time
        current_time=$(date +%s)
        if [ $((current_time - start_time)) -ge "$timeout" ]; then
            echo "Timeout waiting for admin version ${expected}"
            return 1
        fi
        sleep 0.5
    done
}

assert_eq() {
    local expected="$1"
    local actual="$2"
    local msg="$3"
    if [ "$actual" != "$expected" ]; then
        echo "❌ Assertion failed: $msg"
        echo "   Expected: '$expected'"
        echo "   Actual:   '$actual'"
        exit 1
    fi
}

wait_for_log_match() {
    local pattern="$1"
    local timeout="${2:-10}"
    local log_file="${3:-$TEST_TMP/logs/pavis.log}"
    local retries=$((timeout * 4))
    local backoff=0.25
    
            for _ in $(seq 1 $retries); do
    
                if [ -f "$log_file" ]; then
    
                     if grep -qE "$pattern" "$log_file"; then
    
                        return 0
    
                     fi
    
                fi
    
                sleep "$backoff"
    
            done
    
        
    
    
    return 1
}

assert_metric_at_least() {
    local pattern="$1"
    local min="${2:-1}"
    local timeout="${3:-10}"
    local metrics_url="${4:-http://127.0.0.1:$PORT_METRICS}"
    local retries=$((timeout * 4))
    local backoff=0.25
    
            for _ in $(seq 1 $retries); do
    
                metrics=$(curl -s --connect-timeout 1 --max-time 2 "$metrics_url")
    
                line=$(echo "$metrics" | grep -E "$pattern" | head -n 1)
    
                if [ -n "$line" ]; then
    
                    value=$(echo "$line" | awk '{print $2}')
    
                    echo "DEBUG: Checking metric line: '$line' extracted value: '$value' against min: '$min'"
    
                    if awk -v v="$value" -v min="$min" 'BEGIN {exit !(v >= min)}'; then
    
                        echo "DEBUG: Assertion passed."
    
                        return 0
    
                    else
    
                        echo "DEBUG: Assertion failed (value < min)."
    
                    fi
    
                else
    
                     echo "DEBUG: Metric pattern '$pattern' not found in metrics."
    
                fi
    
                sleep "$backoff"
    
            done
    
            echo "DEBUG: Metric assertion failed for pattern '$pattern'. Last metrics fetch:"
    
            curl -s "$metrics_url"
    
            return 1
    
        }
    
        
    
    

get_relay_config_version() {
    local relay_url="$1"
    local headers_file="${2:-$TEST_TMP/relay.headers}"

    pavis_curl_headers "$headers_file" "${relay_url}/v1/config"
    header_value "$headers_file" "x-config-version"
}

get_runtime_config_version() {
    local metrics_url="$1"
    local metrics
    metrics=$(curl -s --connect-timeout 1 --max-time 2 "$metrics_url" | tr -d '\r') || return 1
    printf '%s\n' "$metrics" | awk '
        match($0, /pavis_runtime_config_version{[^}]*version="[^"]+"/) {
            value=substr($0, RSTART, RLENGTH)
            sub(/^.*version="/, "", value)
            sub( /"$/, "", value)
            print value
            found=1
            exit
        }
        END { if (!found) exit 1 }
    '
}

wait_for_runtime_config_version() {
    local metrics_url="$1"
    local expected_version="$2"
    local timeout="${3:-10}"
    local retries=$((timeout * 4))
    local backoff=0.25

    echo "DEBUG: wait_for_runtime_config_version looking for '$expected_version' at $metrics_url" >&2

    for i in $(seq 1 $retries); do
        local metrics
        metrics=$(curl -s --connect-timeout 1 --max-time 2 "$metrics_url" | tr -d '\r') || true
        
        if [ -n "$expected_version" ]; then
            if echo "$metrics" | grep -q "pavis_runtime_config_version{version=\"$expected_version\"}"; then
                echo "DEBUG: Found version $expected_version at retry $i" >&2
                echo "$expected_version"
                return 0
            fi
            if [ $((i % 4)) -eq 0 ]; then
                echo "DEBUG: Retry $i, version $expected_version not found yet. Current metrics (subset):" >&2
                echo "$metrics" | grep "pavis_runtime_config_version" | head -n 5 >&2
            fi
        else
            # If no specific version expected, return the first one found
            local version
            version=$(echo "$metrics" | awk '
                match($0, /pavis_runtime_config_version{[^}]*version="[^"]+"/) {
                    value=substr($0, RSTART, RLENGTH)
                    sub(/^.*version="/, "", value)
                    sub( /"$/, "", value)
                    print value
                    found=1
                    exit
                }
                END { if (!found) exit 1 }
            ')
            if [ -n "$version" ]; then
                echo "$version"
                return 0
            fi
        fi
        sleep "$backoff"
    done
    return 1
}

json_get_string() {
    local key="$1"
    awk -v key="$key" '
        { gsub(/\r|\n/, "", $0) }
        match($0, "\"" key "\"[[:space:]]*:[[:space:]]*\"[^\"]*\"") {
            value=substr($0, RSTART, RLENGTH)
            sub(/.*:[[:space:]]*"/, "", value)
            sub( /"$/, "", value)
            print value
            found=1
            exit
        }
        END { if (!found) exit 1 }
    '
}

json_get_number() {
    local key="$1"
    awk -v key="$key" '
        { gsub(/\r|\n/, "", $0) }
        match($0, "\"" key "\"[[:space:]]*:[[:space:]]*[0-9]+") {
            value=substr($0, RSTART, RLENGTH)
            sub(/.*:[[:space:]]*/, "", value)
            print value
            found=1
            exit
        }
        END { if (!found) exit 1 }
    '
}

json_get_bool() {
    local key="$1"
    awk -v key="$key" '
        { gsub(/\r|\n/, "", $0) }
        match($0, "\"" key "\"[[:space:]]*:[[:space:]]*(true|false)") {
            value=substr($0, RSTART, RLENGTH)
            sub(/.*:[[:space:]]*/, "", value)
            print value
            found=1
            exit
        }
        END { if (!found) exit 1 }
    '
}

json_get_header_first() {
    local header="$1"
    awk -v header="$header" '
        { gsub(/\r|\n/, "", $0) }
        match($0, "\"" header "\"[[:space:]]*:[[:space:]]*\\[\"[^\"]*\"") {
            value=substr($0, RSTART, RLENGTH)
            pos=index(value, "[\"")
            if (pos > 0) {
                value=substr(value, pos + 2)
            }
            sub( /"$/, "", value)
            print value
            found=1
            exit
        }
        END { if (!found) exit 1 }
    '
}

json_get_header_joined() {
    local header="$1"
    awk -v header="$header" '
        { gsub(/\r|\n/, "", $0) }
        match($0, "\"" header "\"[[:space:]]*:[[:space:]]*\\[[^]]*\\]") {
            value=substr($0, RSTART, RLENGTH)
            start=index(value, "[")
            end=index(value, "]")
            if (start > 0 && end > start) {
                value=substr(value, start + 1, end - start - 1)
            }
            gsub( /"/, "", value)
            gsub(/[[:space:]]+/, "", value)
            gsub(/,/, ", ", value)
            print value
            found=1
            exit
        }
        END { if (!found) exit 1 }
    '
}

json_get_tls_bool() {
    local key="$1"
    awk -v key="$key" '
        { gsub(/\r|\n/, "", $0) }
        match($0, "\"tls\"[^}]*\"" key "\"[[:space:]]*:[[:space:]]*(true|false)") {
            value=substr($0, RSTART, RLENGTH)
            sub(/.*:[[:space:]]*/, "", value)
            print value
            found=1
            exit
        }
        END { if (!found) exit 1 }
    '
}

json_get_tls_string() {
    local key="$1"
    awk -v key="$key" '
        { gsub(/\r|\n/, "", $0) }
        match($0, "\"tls\"[^}]*\"" key "\"[[:space:]]*:[[:space:]]*\"[^\"]*\"") {
            value=substr($0, RSTART, RLENGTH)
            sub(/.*:[[:space:]]*"/, "", value)
            sub( /"$/, "", value)
            print value
            found=1
            exit
        }
        END { if (!found) exit 1 }
    '
}

# --- P2 Extension Helpers ---

assert_ge() {
    local actual="$1"
    local expected="$2"
    local msg="$3"
    if [ "$(awk "BEGIN {print ($actual >= $expected)}")" -eq 0 ]; then
        echo "❌ Assertion failed: $msg"
        echo "   Expected: >= $expected"
        echo "   Actual:   $actual"
        exit 1
    fi
}

assert_le() {
    local actual="$1"
    local expected="$2"
    local msg="$3"
    if [ "$(awk "BEGIN {print ($actual <= $expected)}")" -eq 0 ]; then
        echo "❌ Assertion failed: $msg"
        echo "   Expected: <= $expected"
        echo "   Actual:   $actual"
        exit 1
    fi
}

# Get current metric value
get_metric() {
    local metric_pattern="$1"
    curl -s "http://127.0.0.1:${PORT_PAVIS:-8080}/metrics" | \
        grep -E "$metric_pattern" | \
        head -1 | \
        awk '{print $2}'
}

# Assert metric >= threshold
assert_metric_ge() {
    local metric="$1"
    local threshold="$2"
    local msg="${3:-Metric $metric should be >= $threshold}"
    local value
    value=$(get_metric "$metric")
    [ -z "$value" ] && value=0
    assert_ge "$value" "$threshold" "$msg"
}

# Assert metric <= threshold (for gauge invariants like pool.max)
assert_metric_le() {
    local metric="$1"
    local threshold="$2"
    local msg="${3:-Metric $metric should be <= $threshold}"
    local value
    value=$(get_metric "$metric")
    [ -z "$value" ] && value=0
    assert_le "$value" "$threshold" "$msg"
}

# Poll metric multiple times, return max value (for gauge invariants)
get_metric_max() {
    local metric="$1"
    local samples="${2:-10}"
    local delay="${3:-0.1}"
    local max=0

    for _ in $(seq 1 "$samples"); do
        local current
        current=$(get_metric "$metric")
        [ -z "$current" ] && current=0
        if [ "$(awk "BEGIN {print ($current > $max)}")" -eq 1 ]; then
            max=$current
        fi
        sleep "$delay"
    done
    echo "$max"
}

# Wait for config reload to complete

wait_for_reload() {

    local timeout="${1:-5}"

    sleep 1  # Give relay time to process

    wait_for_url "http://127.0.0.1:${PORT_PAVIS:-8080}/healthz" "$timeout"

}



fail() {
    echo "❌ $1" >&2
    exit 1
}

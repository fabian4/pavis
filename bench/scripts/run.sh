#!/bin/bash
set -euo pipefail

# Benchmark Runner
# ================
# Runs benchmark matrix for a single proxy.
# Usage: BENCHMARK_TARGET=pavis bash run.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_DIR="$(dirname "$SCRIPT_DIR")"
RESULTS_DIR="${RESULTS_DIR:-${BENCH_DIR}/output}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

THREADS=${THREADS:-4}
WARMUP=${WARMUP:-5s}
BENCHMARK_TARGET=${BENCHMARK_TARGET:-pavis}

# Proxy Versions (defaults for local run)
export PAVIS_TAG="${PAVIS_TAG:-bench}"
export ENVOY_TAG="${ENVOY_TAG:-v1.32-latest}"
export NGINX_TAG="${NGINX_TAG:-alpine}"
export HAPROXY_TAG="${HAPROXY_TAG:-2.9-alpine}"

# Helper to get version for the current target
get_version() {
    case $1 in
        pavis) echo "${PAVIS_TAG}" ;;
        envoy) echo "${ENVOY_TAG}" ;;
        nginx) echo "${NGINX_TAG}" ;;
        haproxy) echo "${HAPROXY_TAG}" ;;
        *) echo "unknown" ;;
    esac
}

PROXY_VERSION=$(get_version "$BENCHMARK_TARGET")

# Proxy port mapping
declare -A PROXY_PORTS=(
    ["pavis"]="8080"
    ["envoy"]="8081"
    ["nginx"]="8082"
    ["haproxy"]="8083"
)

if [[ ! -v PROXY_PORTS[$BENCHMARK_TARGET] ]]; then
    echo "ERROR: Unknown proxy '${BENCHMARK_TARGET}'. Use: pavis, envoy, nginx, haproxy"
    exit 1
fi

PROXY_PORT="${PROXY_PORTS[$BENCHMARK_TARGET]}"
PROXY_URL="http://localhost:${PROXY_PORT}"

# Output directory for this specific target
TARGET_DIR="${RESULTS_DIR}/${BENCHMARK_TARGET}"
OUTPUT_FILE="${TARGET_DIR}/${BENCHMARK_TARGET}.txt"
LOG_DIR="${TARGET_DIR}/logs"

mkdir -p "${TARGET_DIR}"
mkdir -p "${LOG_DIR}"

# Initialize output file with header
cat > "$OUTPUT_FILE" << EOF
# Benchmark Results: ${BENCHMARK_TARGET}
# Version: ${PROXY_VERSION}
# Generated: $(date -u '+%Y-%m-%dT%H:%M:%SZ')

EOF

echo "=============================================="
echo "  Benchmark: ${BENCHMARK_TARGET}"
echo "=============================================="
echo "Version:  ${PROXY_VERSION}"
echo "Port:     ${PROXY_PORT}"
echo "Output:   ${OUTPUT_FILE}"
echo "=============================================="

# Check wrk is installed
if command -v wrk2 &> /dev/null; then
    WRK_CMD="wrk2"
elif command -v wrk &> /dev/null; then
    WRK_CMD="wrk"
else
    echo "ERROR: wrk not found. Install with:"
    echo "  Ubuntu: sudo apt install wrk"
    echo "  macOS:  brew install wrk"
    exit 1
fi

echo "Using: ${WRK_CMD}"

# Check ulimit
ULIMIT=$(ulimit -n)
echo "Current ulimit -n: $ULIMIT"
if [ "$ULIMIT" -ne "unlimited" ] && [ "$ULIMIT" -lt 10000 ]; then
    echo "WARNING: ulimit -n is $ULIMIT. Tests with 10000 connections may fail."
fi

# Wait for service to be ready
wait_for_service() {
    local max_attempts=30
    local attempt=0
    
    echo "Waiting for ${BENCHMARK_TARGET}..."
    while ! curl -sf "${PROXY_URL}/get" > /dev/null 2>&1; do
        attempt=$((attempt + 1))
        if [ $attempt -ge $max_attempts ]; then
            echo "ERROR: ${BENCHMARK_TARGET} not ready after ${max_attempts} attempts"
            return 1
        fi
        sleep 1
    done
    echo "${BENCHMARK_TARGET} is ready"
}

# Start containers with specific resource limits
start_containers() {
    local cpu_cores=$1
    local memory_mib=$2

    if [ "$BENCHMARK_TARGET" = "pavis" ]; then
        echo "Compiling Pavis configuration..."
        cargo run -p pavctl -- gen "$BENCH_DIR/config/pavis.yaml" "$BENCH_DIR/config/pavis.pvs"
    fi

    echo "Starting ${BENCHMARK_TARGET}: CPU=${cpu_cores}, Memory=${memory_mib}MiB"
    
    cd "$BENCH_DIR"
    docker compose down -v 2>/dev/null || true
    CPU_LIMIT="${cpu_cores}" MEMORY_LIMIT="${memory_mib}M" \
        docker compose up -d backend "${BENCHMARK_TARGET}"
    
    wait_for_service
}

# Stop containers
stop_containers() {
    cd "$BENCH_DIR"
    docker compose down -v 2>/dev/null || true
}

# Run a single benchmark
run_benchmark() {
    local run_id=$1
    local connections=$2
    local duration_sec=$3
    local use_churn=$4
    
    echo ""
    echo "----------------------------------------------"
    echo "  ${BENCHMARK_TARGET}: ${run_id}"
    echo "  Connections: ${connections}, Duration: ${duration_sec}s"
    echo "----------------------------------------------"
    
    # Write header to consolidated output
    {
        echo "========================================"
        echo "Config: ${run_id}"
        echo "========================================"
    } >> "$OUTPUT_FILE"
    
    # Warmup
    $WRK_CMD -t2 -c50 -d${WARMUP} "${PROXY_URL}/get" > /dev/null 2>&1 || true
    
    # Start background stats collection
    local temp_stats=$(mktemp)
    (
        while true; do
            docker stats "bench-${BENCHMARK_TARGET}" --no-stream --format "{{.CPUPerc}},{{.MemUsage}}" >> "$temp_stats" 2>/dev/null || true
            sleep 1
        done
    ) &
    local stats_pid=$!
    
    # Run benchmark
    if [ "$use_churn" = "true" ]; then
        $WRK_CMD -t${THREADS} -c${connections} -d${duration_sec}s --latency -H "Connection: close" "${PROXY_URL}/get" 2>&1 >> "$OUTPUT_FILE"
    else
        $WRK_CMD -t${THREADS} -c${connections} -d${duration_sec}s --latency "${PROXY_URL}/get" 2>&1 >> "$OUTPUT_FILE"
    fi
    
    # Stop stats collection
    kill $stats_pid 2>/dev/null || true
    wait $stats_pid 2>/dev/null || true
    
    {
        echo "----------------------------------------"
        echo "Resource Stats:"
        cat "$temp_stats"
        rm "$temp_stats"
    } >> "$OUTPUT_FILE"
    
    # Dump logs to separate file
    local log_file="${LOG_DIR}/${run_id}.log"
    docker logs "bench-${BENCHMARK_TARGET}" > "$log_file" 2>&1
    
    echo "" >> "$OUTPUT_FILE"
}

# Run CI matrix (4 runs)
run_ci_matrix() {
    echo ""
    echo "=============================================="
    echo "  CI Matrix: ${BENCHMARK_TARGET}"
    echo "=============================================="
    
    start_containers 2 512
    run_benchmark "throughput_baseline_short_1x" 100 30 false
    run_benchmark "latency_baseline_short_1x" 500 30 false
    run_benchmark "concurrency_baseline_short_1x" 5000 30 false
    run_benchmark "churn_baseline_short_1x" 100 30 true
    stop_containers
}

# Run extended matrix (7 runs)
run_extended_matrix() {
    echo ""
    echo "=============================================="
    echo "  Extended Matrix: ${BENCHMARK_TARGET}"
    echo "=============================================="
    
    # Resource variation: cpu-limited
    start_containers 1 512
    run_benchmark "throughput_cpu-limited_short_1x" 100 30 false
    run_benchmark "churn_cpu-limited_short_1x" 100 30 true
    stop_containers
    
    # Resource variation: memory-limited
    start_containers 2 256
    run_benchmark "throughput_memory-limited_short_1x" 100 30 false
    stop_containers
    
    # Duration variation: extended
    start_containers 2 512
    run_benchmark "throughput_baseline_extended_1x" 100 300 false
    run_benchmark "latency_baseline_extended_1x" 500 300 false
    stop_containers
    
    # Intensity variation: 2x
    start_containers 2 512
    run_benchmark "latency_baseline_short_2x" 1000 30 false
    run_benchmark "concurrency_baseline_short_2x" 10000 30 false
    stop_containers
}

# Main execution
run_ci_matrix
run_extended_matrix

echo ""
echo "=============================================="
echo "  ${BENCHMARK_TARGET} Benchmark Complete"
echo "=============================================="
echo "Output: ${OUTPUT_FILE}"
echo "Runs: 11 (4 CI + 7 extended)"

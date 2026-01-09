#!/bin/bash
set -euo pipefail

# Benchmark Runner (Enhanced with Open-Loop, Multi-Run, Backend Selection)
# =========================================================================
# Runs benchmark matrix for a single proxy with methodological improvements:
#   - Open-loop load (wrk2) for latency workloads with fixed target RPS
#   - Closed-loop load (wrk) for throughput/concurrency/churn workloads
#   - Multi-run support (N iterations) for statistical validity
#   - Backend selection (httpbin vs minimal)
#   - CPU pinning (distinct cores for proxy, backend, load generator)
#
# Usage: BENCHMARK_TARGET=pavis bash run.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_DIR="$(dirname "$SCRIPT_DIR")"
RESULTS_DIR="${RESULTS_DIR:-${BENCH_DIR}/output}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

THREADS=${THREADS:-4}
WARMUP=${WARMUP:-5s}
BENCHMARK_TARGET=${BENCHMARK_TARGET:-pavis}
BENCHMARK_RUNS=${BENCHMARK_RUNS:-1}  # Number of repeated runs for statistical validity
BACKEND_TYPE=${BACKEND_TYPE:-httpbin}  # httpbin or minimal

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

# Backend configuration
if [ "$BACKEND_TYPE" = "minimal" ]; then
    BACKEND_IMAGE="bench-minimal-backend:latest"
    BACKEND_PORT="8001"
    BACKEND_INTERNAL_PORT="8000"
    BACKEND_MEMORY="512M"
    BACKEND_HEALTHCHECK='["CMD-SHELL", "wget -q -O- http://localhost:8000/health || exit 1"]'
else
    # Default: httpbin
    BACKEND_IMAGE="kennethreitz/httpbin:latest"
    BACKEND_PORT="8000"
    BACKEND_INTERNAL_PORT="80"
    BACKEND_MEMORY="1G"
    BACKEND_HEALTHCHECK='["CMD-SHELL", "python3 -c '\''import urllib.request; urllib.request.urlopen(\"http://localhost:80/get\")'\''"]'
fi

# Output directory for this specific target
TARGET_DIR="${RESULTS_DIR}/${BENCHMARK_TARGET}"
OUTPUT_FILE="${TARGET_DIR}/${BENCHMARK_TARGET}.txt"
LOG_DIR="${TARGET_DIR}/logs"

mkdir -p "${TARGET_DIR}"
mkdir -p "${LOG_DIR}"

# Initialize output file with header
cat > "$OUTPUT_FILE" <<EOF
# Benchmark Results: ${BENCHMARK_TARGET}
# Version: ${PROXY_VERSION}
# Backend: ${BACKEND_TYPE}
# Generated: $(date -u '+%Y-%m-%dT%H:%M:%SZ')

EOF

echo "=============================================="
echo "  Benchmark: ${BENCHMARK_TARGET}"
echo "=============================================="
echo "Version:  ${PROXY_VERSION}"
echo "Port:     ${PROXY_PORT}"
echo "Backend:  ${BACKEND_TYPE}"
echo "Runs:     ${BENCHMARK_RUNS}"
echo "Output:   ${OUTPUT_FILE}"
echo "=============================================="

# Check wrk/wrk2 availability
WRK_CMD=""
WRK2_CMD=""
if command -v wrk2 &> /dev/null; then
    WRK2_CMD="wrk2"
fi
if command -v wrk &> /dev/null; then
    WRK_CMD="wrk"
fi

if [ -z "$WRK_CMD" ] && [ -z "$WRK2_CMD" ]; then
    echo "ERROR: Neither wrk nor wrk2 found. Install with:"
    echo "  Ubuntu: sudo apt install wrk"
    echo "  macOS:  brew install wrk"
    echo "  wrk2:   https://github.com/giltene/wrk2"
    exit 1
fi

echo "Load generators available:"
[ -n "$WRK_CMD" ] && echo "  - wrk (closed-loop)"
[ -n "$WRK2_CMD" ] && echo "  - wrk2 (open-loop)"

# Check ulimit
ULIMIT=$(ulimit -n)
echo "Current ulimit -n: $ULIMIT"
if [ "$ULIMIT" != "unlimited" ]; then
    if [ "$ULIMIT" -lt 10000 ] 2>/dev/null; then
        echo "WARNING: ulimit -n is $ULIMIT. Tests with 10000 connections may fail."
        echo "         Increase with: ulimit -n 10000"
    fi
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

# Wait for backend to be ready
wait_for_backend() {
    local max_attempts=30
    local attempt=0
    local backend_url=""

    if [ "$BACKEND_TYPE" = "minimal" ]; then
        backend_url="http://localhost:8001/health"
    else
        backend_url="http://localhost:8000/get"
    fi

    echo "Waiting for backend ($BACKEND_TYPE)..."
    while ! curl -sf "$backend_url" > /dev/null 2>&1; do
        attempt=$((attempt + 1))
        if [ $attempt -ge $max_attempts ]; then
            echo "ERROR: Backend not ready after ${max_attempts} attempts"
            return 1
        fi
        sleep 1
    done
    echo "Backend ($BACKEND_TYPE) is ready"
}

# Start containers with specific resource limits
start_containers() {
    local cpu_cores=$1
    local memory_mib=$2

    if [ "$BENCHMARK_TARGET" = "pavis" ]; then
        echo "Compiling Pavis configuration..."
        cargo run -p pavctl -- gen "$BENCH_DIR/config/pavis.yaml" "$BENCH_DIR/config/pavis.pvs"
    fi

    # Build minimal backend if needed
    if [ "$BACKEND_TYPE" = "minimal" ]; then
        echo "Building minimal backend..."
        cd "$BENCH_DIR"
        docker compose build backend-minimal
    fi

    echo "Starting ${BENCHMARK_TARGET}: CPU=${cpu_cores}, Memory=${memory_mib}MiB"

    cd "$BENCH_DIR"
    docker compose down -v 2>/dev/null || true

    # Set CPU pinning: backend on CPU 0, proxy on CPUs 1-2 (or 1 if limited)
    local proxy_cpuset="1-2"
    if [ "$cpu_cores" = "1" ]; then
        proxy_cpuset="1"
    fi

    # Select which backend profile to use
    local backend_profile="httpbin"
    if [ "$BACKEND_TYPE" = "minimal" ]; then
        backend_profile="minimal"
    fi

    CPU_LIMIT="${cpu_cores}" \
    MEMORY_LIMIT="${memory_mib}M" \
    PROXY_CPUSET="${proxy_cpuset}" \
        docker compose --profile "${backend_profile}" up -d "${BENCHMARK_TARGET}"

    wait_for_backend
    wait_for_service
}

# Stop containers
stop_containers() {
    cd "$BENCH_DIR"
    docker compose down -v 2>/dev/null || true
}

# Run a single benchmark iteration
run_benchmark_iteration() {
    local run_id=$1
    local iteration=$2
    local connections=$3
    local duration_sec=$4
    local use_churn=$5
    local load_type=$6
    local target_rps=${7:-0}

    local iteration_label=""
    if [ "$BENCHMARK_RUNS" -gt 1 ]; then
        iteration_label=" [Run ${iteration}/${BENCHMARK_RUNS}]"
    fi

    echo ""
    echo "----------------------------------------------"
    echo "  ${BENCHMARK_TARGET}: ${run_id}${iteration_label}"
    echo "  Load: ${load_type} | Connections: ${connections} | Duration: ${duration_sec}s"
    if [ "$load_type" = "open-loop" ] && [ "$target_rps" -gt 0 ]; then
        echo "  Target RPS: ${target_rps}"
    fi
    echo "----------------------------------------------"

    # Select load generator based on load type
    local load_cmd=""
    if [ "$load_type" = "open-loop" ]; then
        if [ -z "$WRK2_CMD" ]; then
            echo "WARNING: Open-loop requested but wrk2 not available. Falling back to wrk (closed-loop)."
            load_cmd="$WRK_CMD"
        else
            load_cmd="$WRK2_CMD"
        fi
    else
        load_cmd="${WRK_CMD:-$WRK2_CMD}"
    fi

    # Write header to consolidated output (only for first iteration)
    if [ "$iteration" -eq 1 ]; then
        {
            echo "========================================"
            echo "Config: ${run_id}"
            echo "Load Type: ${load_type}"
            echo "Backend: ${BACKEND_TYPE}"
            echo "Runs: ${BENCHMARK_RUNS}"
            echo "========================================"
        } >> "$OUTPUT_FILE"
    fi

    # Iteration-specific header
    {
        echo ""
        echo "--- Iteration ${iteration}/${BENCHMARK_RUNS} ---"
    } >> "$OUTPUT_FILE"

    # Warmup
    $load_cmd -t2 -c50 -d${WARMUP} "${PROXY_URL}/get" > /dev/null 2>&1 || true

    # Start background stats collection
    local temp_stats=$(mktemp)
    (
        while true; do
            docker stats "bench-${BENCHMARK_TARGET}" --no-stream --format "{{.CPUPerc}},{{.MemUsage}}" >> "$temp_stats" 2>/dev/null || true
            docker stats "bench-backend" --no-stream --format "{{.CPUPerc}},{{.MemUsage}}" >> "$temp_stats.backend" 2>/dev/null || true
            sleep 1
        done
    ) &
    local stats_pid=$!

    # Run benchmark
    local bench_result=""
    if [ "$use_churn" = "true" ]; then
        bench_result=$($load_cmd -t${THREADS} -c${connections} -d${duration_sec}s --latency -H "Connection: close" "${PROXY_URL}/get" 2>&1)
    elif [ "$load_type" = "open-loop" ] && [ "$target_rps" -gt 0 ] && [ -n "$WRK2_CMD" ]; then
        # Open-loop with wrk2
        bench_result=$($WRK2_CMD -t${THREADS} -c${connections} -d${duration_sec}s -R${target_rps} --latency "${PROXY_URL}/get" 2>&1)
    else
        # Closed-loop
        bench_result=$($load_cmd -t${THREADS} -c${connections} -d${duration_sec}s --latency "${PROXY_URL}/get" 2>&1)
    fi

    # Stop stats collection
    kill $stats_pid 2>/dev/null || true
    wait $stats_pid 2>/dev/null || true

    # Write results
    {
        echo "$bench_result"
        echo "----------------------------------------"
        echo "Proxy Resource Stats:"
        cat "$temp_stats"
        echo "----------------------------------------"
        echo "Backend Resource Stats:"
        cat "$temp_stats.backend"
        rm -f "$temp_stats" "$temp_stats.backend"
    } >> "$OUTPUT_FILE"

    # Dump logs to separate file (only for first iteration)
    if [ "$iteration" -eq 1 ]; then
        local log_file="${LOG_DIR}/${run_id}.log"
        docker logs "bench-${BENCHMARK_TARGET}" > "$log_file" 2>&1
    fi
}

# Run a benchmark configuration (with multi-run support)
run_benchmark() {
    local run_id=$1
    local connections=$2
    local duration_sec=$3
    local use_churn=$4
    local load_type=${5:-closed-loop}
    local target_rps=${6:-0}
    local num_runs=${7:-1}

    # Run multiple iterations
    for i in $(seq 1 $num_runs); do
        run_benchmark_iteration "$run_id" "$i" "$connections" "$duration_sec" "$use_churn" "$load_type" "$target_rps"

        # Short cooldown between runs (except for last run)
        if [ "$i" -lt "$num_runs" ]; then
            echo "Cooldown (5s)..."
            sleep 5
        fi
    done

    echo "" >> "$OUTPUT_FILE"
}

# Run CI matrix (4 runs)
run_ci_matrix() {
    echo ""
    echo "=============================================="
    echo "  CI Matrix: ${BENCHMARK_TARGET}"
    echo "=============================================="

    start_containers 2 512
    run_benchmark "throughput_baseline_short_1x" 100 30 false "closed-loop" 0 1
    run_benchmark "latency_baseline_short_1x" 500 30 false "open-loop" 10000 1
    run_benchmark "concurrency_baseline_short_1x" 5000 30 false "closed-loop" 0 1
    run_benchmark "churn_baseline_short_1x" 100 30 true "closed-loop" 0 1
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
    run_benchmark "throughput_cpu-limited_short_1x" 100 30 false "closed-loop" 0 1
    run_benchmark "churn_cpu-limited_short_1x" 100 30 true "closed-loop" 0 1
    stop_containers

    # Resource variation: memory-limited
    start_containers 2 256
    run_benchmark "throughput_memory-limited_short_1x" 100 30 false "closed-loop" 0 1
    stop_containers

    # Duration variation: extended (with minimal backend for dataplane isolation)
    ORIGINAL_BACKEND=$BACKEND_TYPE
    BACKEND_TYPE="minimal"
    start_containers 2 512
    run_benchmark "throughput_baseline_extended_1x" 100 300 false "closed-loop" 0 1
    run_benchmark "latency_baseline_extended_1x" 500 300 false "open-loop" 10000 5  # Multi-run for statistical validity
    stop_containers
    BACKEND_TYPE=$ORIGINAL_BACKEND

    # Intensity variation: 2x
    start_containers 2 512
    run_benchmark "latency_baseline_short_2x" 1000 30 false "open-loop" 20000 1
    run_benchmark "concurrency_baseline_short_2x" 10000 30 false "closed-loop" 0 1
    stop_containers
}

# Run Pavis-specific benchmarks (optional, only for Pavis)
run_pavis_specific() {
    if [ "$BENCHMARK_TARGET" != "pavis" ]; then
        echo "Skipping Pavis-specific benchmarks (target: ${BENCHMARK_TARGET})"
        return
    fi

    echo ""
    echo "=============================================="
    echo "  Pavis-Specific Benchmarks"
    echo "=============================================="

    # Hot-reload jitter test
    ORIGINAL_BACKEND=$BACKEND_TYPE
    BACKEND_TYPE="minimal"
    start_containers 2 512

    echo "Running reload benchmark (hot-reload jitter test)..."
    # This requires a special script to trigger reloads during the benchmark
    # For now, we'll run a standard latency test with a note
    # TODO: Implement actual reload triggering mechanism
    run_benchmark "reload_baseline_short_1x" 500 30 false "open-loop" 5000 5

    stop_containers
    BACKEND_TYPE=$ORIGINAL_BACKEND

    echo "NOTE: Reload benchmark ran as standard latency test."
    echo "      Implement reload triggering for full hot-reload jitter analysis."
}

# Main execution
run_ci_matrix
run_extended_matrix

# Optional: Run Pavis-specific benchmarks
if [ "${RUN_PAVIS_SPECIFIC:-false}" = "true" ]; then
    run_pavis_specific
fi

echo ""
echo "=============================================="
echo "  ${BENCHMARK_TARGET} Benchmark Complete"
echo "=============================================="
echo "Output: ${OUTPUT_FILE}"
echo "Runs: 11 (4 CI + 7 extended)"
echo "Backend: ${BACKEND_TYPE}"
echo "Multi-run: ${BENCHMARK_RUNS} iteration(s) per config"

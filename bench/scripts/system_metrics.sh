#!/usr/bin/env bash
set -euo pipefail

# System Mode Metrics Collection Helpers
# Provides functions for collecting convergence time, RSS, FD count, etc.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"
# shellcheck source=bench/scripts/k8s_helpers.sh
source "$SCRIPT_DIR/k8s_helpers.sh"

# Measure config convergence time
# Usage: collect_convergence_time <expected_version>
# Returns: convergence time in milliseconds
collect_convergence_time() {
  local expected_version="$1"
  local start_ms
  local end_ms
  local max_wait_ms="${2:-60000}"  # Default 60s timeout

  start_ms=$(date +%s%3N)
  local elapsed_ms=0

  while [[ $elapsed_ms -lt $max_wait_ms ]]; do
    # Query current config version from pavis sidecar
    local current_version
    current_version=$(detect_config_version "app=test-backend" "pavis-sidecar") || true

    if [[ "$current_version" == "$expected_version" ]]; then
      end_ms=$(date +%s%3N)
      echo $((end_ms - start_ms))
      return 0
    fi

    sleep 0.1
    end_ms=$(date +%s%3N)
    elapsed_ms=$((end_ms - start_ms))
  done

  log_error "Timeout waiting for config version $expected_version"
  return 1
}

# Detect active config version
# Usage: detect_config_version <pod_label> <container_name>
# Returns: config version number
detect_config_version() {
  local label="$1"
  local container="$2"
  local namespace="${3:-${BENCH_NAMESPACE:-bench-system}}"

  if [[ "$container" == "pavis-sidecar" ]]; then
    local version
    local exec_status
    set +e
    local pod_name
    pod_name=$(kubectl_get_pod_name "$label" "$namespace")
    version=$(kubectl exec -n "$namespace" "$pod_name" -c "$container" -- \
      cat /config/bootstrap.pvs.version 2>/dev/null | tr -d '\r\n')
    exec_status=$?
    set -e
    if (( exec_status == 0 )) && [[ -n "$version" ]]; then
      echo "$version"
      return 0
    fi
  fi

  # Query sidecar via admin endpoint or logs
  # For now, use a simple HTTP call to a debug endpoint
  local pod_ip
  pod_ip=$(kubectl_get_pod_ip "$label" "$namespace")

  # Assume admin exposes /admin/config_version endpoint
  curl -s "http://${pod_ip}:9090/admin/config_version" 2>/dev/null || echo "unknown"
}

# Collect RSS memory usage timeline
# Usage: collect_rss_timeline <duration_seconds> <interval_seconds> <output_file>
collect_rss_timeline() {
  local duration_s="$1"
  local interval_s="$2"
  local output_file="$3"
  local label="${4:-app=test-backend}"
  local container="${5:-pavis-sidecar}"
  local namespace="${6:-${BENCH_NAMESPACE:-bench-system}}"

  local elapsed=0
  echo "timestamp_s,rss_kb" > "$output_file"

  while [[ $elapsed -lt $duration_s ]]; do
    local rss_kb
    rss_kb=$(kubectl_get_container_stats "$label" "$container" "$namespace" | tr -d 'Ki' || echo "0")

    local timestamp
    timestamp=$(date +%s)

    echo "${timestamp},${rss_kb}" >> "$output_file"

    sleep "$interval_s"
    elapsed=$((elapsed + interval_s))
  done

  log_info "RSS timeline saved to $output_file"
}

# Calculate RSS slope (MB/hour) using linear regression
# Usage: calculate_rss_slope <timeline_csv>
# Returns: slope in MB/hour
calculate_rss_slope() {
  local timeline_csv="$1"

  # Simple linear regression using awk
  awk -F',' '
    NR > 1 {
      x[NR-1] = ($1 - start_time) / 3600.0  # Convert to hours
      y[NR-1] = $2 / 1024.0                  # Convert to MB
      n++
      if (NR == 2) start_time = $1
    }
    END {
      if (n < 2) {
        print "0"
        exit
      }

      # Calculate means
      sum_x = 0
      sum_y = 0
      for (i = 1; i <= n; i++) {
        sum_x += x[i]
        sum_y += y[i]
      }
      mean_x = sum_x / n
      mean_y = sum_y / n

      # Calculate slope
      numerator = 0
      denominator = 0
      for (i = 1; i <= n; i++) {
        numerator += (x[i] - mean_x) * (y[i] - mean_y)
        denominator += (x[i] - mean_x) * (x[i] - mean_x)
      }

      if (denominator == 0) {
        print "0"
      } else {
        slope = numerator / denominator
        printf "%.2f\n", slope
      }
    }
  ' "$timeline_csv"
}

# Collect file descriptor count
# Usage: collect_fd_count <label> <container>
# Returns: number of open file descriptors
collect_fd_count() {
  local label="$1"
  local container="$2"
  local namespace="${3:-${BENCH_NAMESPACE:-bench-system}}"

  kubectl_get_fd_count "$label" "$container" "$namespace"
}

# Collect FD timeline
# Usage: collect_fd_timeline <duration_seconds> <interval_seconds> <output_file>
collect_fd_timeline() {
  local duration_s="$1"
  local interval_s="$2"
  local output_file="$3"
  local label="${4:-app=test-backend}"
  local container="${5:-pavis-sidecar}"
  local namespace="${6:-${BENCH_NAMESPACE:-bench-system}}"

  local elapsed=0
  echo "timestamp_s,fd_count" > "$output_file"

  while [[ $elapsed -lt $duration_s ]]; do
    local fd_count
    fd_count=$(collect_fd_count "$label" "$container" "$namespace" || echo "0")

    local timestamp
    timestamp=$(date +%s)

    echo "${timestamp},${fd_count}" >> "$output_file"

    sleep "$interval_s"
    elapsed=$((elapsed + interval_s))
  done

  log_info "FD timeline saved to $output_file"
}

# Capture P99 latency snapshot from current load
# Usage: capture_p99_snapshot <duration_seconds>
# Returns: P99 latency in milliseconds
capture_p99_snapshot() {
  local duration_s="${1:-5}"
  local target_url="${2:-http://localhost:8080/fixed}"
  local rps="${3:-1000}"

  local temp_output
  temp_output=$(mktemp)

  # Run short load test to capture latency
  "${BENCH_LOADGEN_BIN}" \
    --url "$target_url" \
    --rate "$rps" \
    --duration "$duration_s" \
    --connections 100 \
    --output "$temp_output" \
    > /dev/null 2>&1

  # Extract P99 from JSON output
  local p99
  p99=$(jq -r '.latency_ms.p99' "$temp_output" 2>/dev/null || echo "0")

  rm -f "$temp_output"
  echo "$p99"
}

# Wait for config to become active
# Usage: wait_for_config_active <expected_version> <timeout_seconds>
wait_for_config_active() {
  local expected_version="$1"
  local timeout_s="${2:-60}"
  local label="${3:-app=test-backend}"
  local container="${4:-pavis-sidecar}"

  local elapsed=0
  while [[ $elapsed -lt $timeout_s ]]; do
    local current_version
    current_version=$(detect_config_version "$label" "$container") || true

    if [[ "$current_version" == "$expected_version" ]]; then
      log_info "Config version $expected_version active"
      return 0
    fi

    sleep 1
    elapsed=$((elapsed + 1))
  done

  log_error "Timeout waiting for config version $expected_version"
  return 1
}

# Get current timestamp in milliseconds
timestamp_ms() {
  date +%s%3N
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  log_info "system_metrics.sh loaded"
fi

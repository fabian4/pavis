#!/usr/bin/env bash
set -euo pipefail

# Kubernetes Helper Functions
# Provides kubectl wrappers and utilities for system mode benchmarks

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"
# Source shared primitives
source "$SCRIPT_DIR/../../scripts/lib/process.sh"

# Wait for pod to be ready
kubectl_wait_ready() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"
  local timeout="${3:-120s}"

  kubectl wait --for=condition=ready pod \
    -l "$label" \
    -n "$namespace" \
    --timeout="$timeout" \
    > /dev/null
}

# Get pod IP address
kubectl_get_pod_ip() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  local pod_ip
  pod_ip=$(kubectl get pod -l "$label" -n "$namespace" \
    -o json 2>/dev/null | jq -r '.items[0].status.podIP // empty' 2>/dev/null || true)
  if [[ -z "$pod_ip" ]]; then
    log_error "No pod IP found for label '$label' in namespace '$namespace'"
    return 1
  fi
  echo "$pod_ip"
}

# Get pod name by label
kubectl_get_pod_name() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  local pod_name
  pod_name=$(kubectl get pod -l "$label" -n "$namespace" \
    -o json 2>/dev/null | jq -r '.items[0].metadata.name // empty' 2>/dev/null || true)
  if [[ -z "$pod_name" ]]; then
    log_error "No pod found for label '$label' in namespace '$namespace'"
    return 1
  fi
  echo "$pod_name"
}

# Start port forward in background
# Returns PID of port-forward process
kubectl_port_forward_background() {
  local label="$1"
  local local_port="$2"
  local remote_port="$3"
  local namespace="${4:-${BENCH_NAMESPACE:-bench-system}}"

  if ! kubectl_wait_ready "$label" "$namespace" 120s; then
    log_error "Timed out waiting for pod readiness for label '$label' in namespace '$namespace'"
    return 1
  fi

  local pod_name
  pod_name=$(kubectl_get_pod_name "$label" "$namespace") || return 1

  local attempt=0
  local max_attempts=5
  local chosen_port="$local_port"
  local pf_pid=""

  while [[ $attempt -lt $max_attempts ]]; do
    if [[ $attempt -gt 0 ]]; then
      chosen_port=$(pick_free_port "$local_port")
    fi

    # Start port-forward in background and redirect output
    kubectl port-forward -n "$namespace" "$pod_name" "$chosen_port:$remote_port" \
      > /dev/null 2>&1 &
    pf_pid=$!

    # Give port-forward time to establish
    sleep 2

    # Verify it's still running using check_process_alive
    if check_process_alive "$pf_pid"; then
      echo "$pf_pid $chosen_port"
      return 0
    fi

    attempt=$((attempt + 1))
  done

  log_error "Port forward failed to start"
  return 1
}

pick_free_port() {
  local fallback_port="$1"

  if command -v python3 > /dev/null 2>&1; then
    python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("", 0))
print(s.getsockname()[1])
s.close()
PY
    return 0
  fi

  if command -v python > /dev/null 2>&1; then
    python - <<'PY'
import socket
s = socket.socket()
s.bind(("", 0))
print(s.getsockname()[1])
s.close()
PY
    return 0
  fi

  echo "$fallback_port"
  return 0
}

# Stop port forward by PID
kubectl_stop_port_forward() {
  local pid="$1"

  # Use kill_process_safe from scripts/lib/process.sh
  # Short timeout (5s) since port-forward should stop quickly
  kill_process_safe "$pid" 5 true 2>/dev/null || true
}

# Execute command in specific container of a pod
kubectl_exec_in_container() {
  local label="$1"
  local container="$2"
  local command="$3"
  local namespace="${4:-${BENCH_NAMESPACE:-bench-system}}"

  local pod_name
  pod_name=$(kubectl_get_pod_name "$label" "$namespace") || return 1

  kubectl exec -n "$namespace" "$pod_name" -c "$container" -- sh -c "$command"
}

# Get container stats (RSS memory usage)
kubectl_get_container_stats() {
  local label="$1"
  local container="$2"
  local namespace="${3:-${BENCH_NAMESPACE:-bench-system}}"

  local pod_name
  pod_name=$(kubectl_get_pod_name "$label" "$namespace") || return 1

  # Get memory usage from metrics-server (if installed)
  # Falls back to /proc read, but do not fail benchmarks if the container lacks a shell.
  if kubectl top pod "$pod_name" -n "$namespace" --containers 2>/dev/null | grep -q "$container"; then
    kubectl top pod "$pod_name" -n "$namespace" --containers | awk -v cont="$container" '$2==cont {print $4}'
  else
    # Fallback: read /proc/self/status without requiring a shell in the container.
    local status
    local exec_status
    set +e
    status=$(kubectl exec -n "$namespace" "$pod_name" -c "$container" -- cat /proc/self/status 2>/dev/null)
    exec_status=$?
    set -e
    if (( exec_status != 0 )); then
      log_warn "Unable to read RSS from container (metrics-server missing or kubectl exec failed). Returning 0." >&2
      echo "0"
      return 0
    fi
    echo "$status" | awk '/VmRSS/ {print $2}'
  fi
}

# Get file descriptor count for process in container
kubectl_get_fd_count() {
  local label="$1"
  local container="$2"
  local namespace="${3:-${BENCH_NAMESPACE:-bench-system}}"

  local pod_name
  pod_name=$(kubectl_get_pod_name "$label" "$namespace") || return 1
  local output
  local exec_status
  set +e
  output=$(kubectl exec -n "$namespace" "$pod_name" -c "$container" -- ls /proc/self/fd 2>/dev/null | wc -l)
  exec_status=$?
  set -e
  if (( exec_status != 0 )); then
    log_warn "Unable to read FD count from container (kubectl exec failed). Returning 0." >&2
    echo "0"
    return 0
  fi
  echo "$output"
}

# Check if pod is ready
kubectl_is_pod_ready() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  local ready
  ready=$(kubectl get pod -l "$label" -n "$namespace" \
    -o jsonpath='{.items[0].status.conditions[?(@.type=="Ready")].status}' 2>/dev/null || echo "False")

  [[ "$ready" == "True" ]]
}

# Get logs from container
kubectl_get_logs() {
  local label="$1"
  local container="$2"
  local namespace="${3:-${BENCH_NAMESPACE:-bench-system}}"
  local lines="${4:-100}"

  local pod_name
  pod_name=$(kubectl_get_pod_name "$label" "$namespace") || return 1

  kubectl logs -n "$namespace" "$pod_name" -c "$container" --tail="$lines"
}

# Get all pod IPs matching label
kubectl_get_all_pod_ips() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  kubectl get pods -l "$label" -n "$namespace" \
    -o jsonpath='{.items[*].status.podIP}'
}

# Wait for service endpoint to be ready
kubectl_wait_for_endpoint() {
  local service_name="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"
  local timeout="${3:-60}"

  local elapsed=0
  while [[ $elapsed -lt $timeout ]]; do
    if kubectl get endpoints "$service_name" -n "$namespace" -o json 2>/dev/null \
      | jq -e '([.subsets[]?.addresses[]?.ip] | length) > 0' >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done

  log_error "Timeout waiting for service endpoint: $service_name"
  return 1
}

# Get service cluster IP
kubectl_get_service_ip() {
  local service_name="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  local service_ip
  service_ip=$(kubectl get service "$service_name" -n "$namespace" \
    -o json 2>/dev/null | jq -r '.spec.clusterIP // empty' 2>/dev/null || true)
  if [[ -z "$service_ip" || "$service_ip" == "None" ]]; then
    log_error "No service IP found for service '$service_name' in namespace '$namespace'"
    return 1
  fi
  echo "$service_ip"
}

# Delete resources by label
kubectl_delete_by_label() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  kubectl delete pod -l "$label" -n "$namespace" --ignore-not-found=true --wait=false
}

# Scale deployment
kubectl_scale_deployment() {
  local deployment="$1"
  local replicas="$2"
  local namespace="${3:-${BENCH_NAMESPACE:-bench-system}}"

  kubectl scale deployment "$deployment" -n "$namespace" --replicas="$replicas"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  # Can be sourced or run directly for testing
  log_info "k8s_helpers.sh loaded"
fi

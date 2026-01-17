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
    --timeout="$timeout"
}

# Get pod IP address
kubectl_get_pod_ip() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  kubectl get pod -l "$label" -n "$namespace" \
    -o jsonpath='{.items[0].status.podIP}'
}

# Get pod name by label
kubectl_get_pod_name() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  kubectl get pod -l "$label" -n "$namespace" \
    -o jsonpath='{.items[0].metadata.name}'
}

# Start port forward in background
# Returns PID of port-forward process
kubectl_port_forward_background() {
  local label="$1"
  local local_port="$2"
  local remote_port="$3"
  local namespace="${4:-${BENCH_NAMESPACE:-bench-system}}"

  local pod_name
  pod_name=$(kubectl_get_pod_name "$label" "$namespace")

  # Start port-forward in background and redirect output
  kubectl port-forward -n "$namespace" "$pod_name" "$local_port:$remote_port" \
    > /dev/null 2>&1 &

  local pf_pid=$!

  # Give port-forward time to establish
  sleep 2

  # Verify it's still running using check_process_alive
  if ! check_process_alive "$pf_pid"; then
    log_error "Port forward failed to start"
    return 1
  fi

  echo "$pf_pid"
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
  pod_name=$(kubectl_get_pod_name "$label" "$namespace")

  kubectl exec -n "$namespace" "$pod_name" -c "$container" -- sh -c "$command"
}

# Get container stats (RSS memory usage)
kubectl_get_container_stats() {
  local label="$1"
  local container="$2"
  local namespace="${3:-${BENCH_NAMESPACE:-bench-system}}"

  local pod_name
  pod_name=$(kubectl_get_pod_name "$label" "$namespace")

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
      log_warn "Unable to read RSS from container (missing tools). Returning 0."
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
  pod_name=$(kubectl_get_pod_name "$label" "$namespace")
  local output
  local exec_status
  set +e
  output=$(kubectl exec -n "$namespace" "$pod_name" -c "$container" -- ls /proc/self/fd 2>/dev/null | wc -l)
  exec_status=$?
  set -e
  if (( exec_status != 0 )); then
    log_warn "Unable to read FD count from container (missing tools). Returning 0."
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
  pod_name=$(kubectl_get_pod_name "$label" "$namespace")

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
    if kubectl get endpoints "$service_name" -n "$namespace" -o jsonpath='{.subsets[0].addresses[0].ip}' 2>/dev/null | grep -q "."; then
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

  kubectl get service "$service_name" -n "$namespace" \
    -o jsonpath='{.spec.clusterIP}'
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

# ============================================================================
# Linkerd-specific helpers
# ============================================================================

# Check if pod has linkerd proxy injected
linkerd_is_injected() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  local pod_name
  pod_name=$(kubectl_get_pod_name "$label" "$namespace")

  # Check if linkerd-proxy container exists
  kubectl get pod "$pod_name" -n "$namespace" \
    -o jsonpath='{.spec.containers[*].name}' | grep -q "linkerd-proxy"
}

# Get linkerd proxy container stats (RSS memory)
linkerd_get_proxy_stats() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  kubectl_get_container_stats "$label" "linkerd-proxy" "$namespace"
}

# Get linkerd proxy version
linkerd_get_proxy_version() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  local pod_name
  pod_name=$(kubectl_get_pod_name "$label" "$namespace")

  kubectl get pod "$pod_name" -n "$namespace" \
    -o jsonpath='{.metadata.annotations.linkerd\.io/proxy-version}'
}

# Get linkerd proxy metrics via admin port
linkerd_get_proxy_metrics() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  kubectl_exec_in_container "$label" "linkerd-proxy" \
    "curl -s http://localhost:4191/metrics" "$namespace"
}

# Check linkerd data plane status for a pod
linkerd_check_pod() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  local pod_name
  pod_name=$(kubectl_get_pod_name "$label" "$namespace")

  linkerd check --proxy --namespace "$namespace" --output short 2>&1 | grep -q "$pod_name"
}

# Get linkerd proxy connection count
linkerd_get_connection_count() {
  local label="$1"
  local namespace="${2:-${BENCH_NAMESPACE:-bench-system}}"

  # Query prometheus metrics from linkerd-proxy
  local metrics
  metrics=$(linkerd_get_proxy_metrics "$label" "$namespace")

  # Extract tcp_open_connections metric
  echo "$metrics" | grep "^tcp_open_connections" | awk '{print $2}' | head -1
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  # Can be sourced or run directly for testing
  log_info "k8s_helpers.sh loaded"
fi

#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench/scripts/utils.sh
source "$SCRIPT_DIR/utils.sh"
# shellcheck source=bench/scripts/pretty.sh
source "$SCRIPT_DIR/pretty.sh"

CLUSTER_NAME="${KIND_CLUSTER_NAME:-pavis-bench}"
NAMESPACE="${BENCH_NAMESPACE:-bench-system}"

# ============================================================================
# System Mode (Kubernetes) Functions
# ============================================================================

resolve_docker_build_mode() {
  if [[ -n "${BENCH_DOCKER_BUILD_MODE:-}" ]]; then
    echo "$BENCH_DOCKER_BUILD_MODE"
    return
  fi

  case "${BENCH_PROFILE:-}" in
    github)
      echo "ci"
      return
      ;;
  esac

  case "${IS_CI:-${CI:-}}" in
    1|true|TRUE|yes|YES)
      echo "ci"
      return
      ;;
  esac

  echo "local"
}

should_quiet_build() {
  case "${BENCH_VERBOSE:-0}" in
    1|true|TRUE|yes|YES)
      return 1
      ;;
  esac
  return 0
}

check_kind_requirements() {
  log_info "Checking system mode requirements"

  # Check for kind
  if ! command -v kind > /dev/null 2>&1; then
    exit_with_error "kind not found. Install from: https://kind.sigs.k8s.io/docs/user/quick-start/#installation"
  fi

  # Check for kubectl
  if ! command -v kubectl > /dev/null 2>&1; then
    exit_with_error "kubectl not found. Install from: https://kubernetes.io/docs/tasks/tools/"
  fi

  # Check for docker
  if ! command -v docker > /dev/null 2>&1; then
    exit_with_error "docker not found. Please install Docker Desktop or Docker Engine"
  fi

  # Verify docker is running
  if ! docker info > /dev/null 2>&1; then
    exit_with_error "Docker daemon is not running. Please start Docker"
  fi

  # Check for linkerd CLI if testing linkerd
  if [[ "${BENCH_PROXY:-}" == "linkerd" ]]; then
    if ! command -v linkerd > /dev/null 2>&1; then
      exit_with_error "linkerd CLI not found. Install from: https://linkerd.io/2/getting-started/#step-1-install-the-cli"
    fi
  fi

  log_info "All requirements satisfied (kind, kubectl, docker)"
}

cluster_exists() {
  kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"
}

create_kind_cluster() {
  if cluster_exists; then
    log_info "Kind cluster '$CLUSTER_NAME' already exists, skipping creation"
    return 0
  fi

  log_info "Creating kind cluster: $CLUSTER_NAME"

  local config_file="${BENCH_ROOT}/bench/config/system/kind-config.yaml"

  if [[ ! -f "$config_file" ]]; then
    exit_with_error "Kind config not found: $config_file"
  fi

  kind create cluster --name "$CLUSTER_NAME" --config "$config_file" --wait 120s

  log_info "Kind cluster created successfully"
}

allow_control_plane_scheduling_if_needed() {
  local node_count
  node_count=$(kubectl get nodes --no-headers 2>/dev/null | wc -l | tr -d ' ')

  if [[ -z "$node_count" || "$node_count" -ge 2 ]]; then
    return 0
  fi

  log_warn "Only ${node_count} node detected; allowing workloads on control-plane"
  kubectl taint nodes --all node-role.kubernetes.io/control-plane- 2>/dev/null || true
  kubectl taint nodes --all node-role.kubernetes.io/master- 2>/dev/null || true
}

build_docker_images() {
  log_info "Building Docker images for system mode"
  local build_mode
  build_mode="$(resolve_docker_build_mode)"

  # Build pavis runtime image
  log_info "Building pavis:local image"
  if should_quiet_build; then
    make -C "${BENCH_ROOT}" docker-build IMAGE=pavis MODE="${build_mode}" > /dev/null 2>&1
  else
    make -C "${BENCH_ROOT}" docker-build IMAGE=pavis MODE="${build_mode}"
  fi

  # Build pavis-relay image
  log_info "Building pavis-relay:local image"
  if should_quiet_build; then
    make -C "${BENCH_ROOT}" docker-build IMAGE=relay MODE="${build_mode}" > /dev/null 2>&1
  else
    make -C "${BENCH_ROOT}" docker-build IMAGE=relay MODE="${build_mode}"
  fi

  # Build bench-upstream image
  log_info "Building pavis-bench-upstream:local image"
  if should_quiet_build; then
    make -C "${BENCH_ROOT}" docker-build IMAGE=bench-upstream MODE="${build_mode}" > /dev/null 2>&1
  else
    make -C "${BENCH_ROOT}" docker-build IMAGE=bench-upstream MODE="${build_mode}"
  fi

  log_info "Docker images built successfully"
}

load_images_to_kind() {
  log_info "Loading images into kind cluster"

  kind load docker-image pavis:local --name "$CLUSTER_NAME"
  kind load docker-image pavis-relay:local --name "$CLUSTER_NAME"
  kind load docker-image pavis-bench-upstream:local --name "$CLUSTER_NAME"

  log_info "Images loaded into kind cluster"
}

create_namespace() {
  log_info "Creating namespace: $NAMESPACE"

  kubectl create namespace "$NAMESPACE" --dry-run=client -o yaml | kubectl apply -f -

  log_info "Namespace ready"
}

install_metrics_server() {
  if kubectl get apiservices v1beta1.metrics.k8s.io >/dev/null 2>&1; then
    log_info "metrics-server already available"
    return 0
  fi

  local manifest_url
  manifest_url="${BENCH_METRICS_SERVER_MANIFEST:-https://github.com/kubernetes-sigs/metrics-server/releases/latest/download/components.yaml}"

  log_info "Installing metrics-server from ${manifest_url}"
  if kubectl apply -f "$manifest_url" >/dev/null 2>&1; then
    kubectl -n kube-system patch deployment metrics-server --type='json' \
      -p='[
        {"op":"add","path":"/spec/template/spec/containers/0/args/-","value":"--kubelet-insecure-tls"},
        {"op":"add","path":"/spec/template/spec/containers/0/args/-","value":"--kubelet-preferred-address-types=InternalIP"}
      ]' >/dev/null 2>&1 || true
    log_info "metrics-server install requested"
  else
    log_warn "metrics-server install failed; RSS metrics may be unavailable"
  fi
}

deploy_pavis_infrastructure() {
  log_info "Deploying Pavis control plane and test workloads"

  local manifests_dir="${BENCH_ROOT}/bench/config/system/pavis"

  if [[ ! -d "$manifests_dir" ]]; then
    exit_with_error "Pavis manifests directory not found: $manifests_dir"
  fi

  # Apply all Pavis manifests
  kubectl apply -f "$manifests_dir/" -n "$NAMESPACE"

  # Wait for relay to be ready
  log_info "Waiting for pavis-relay to be ready"
  kubectl wait --for=condition=ready pod -l app=pavis-relay -n "$NAMESPACE" --timeout=300s

  # Wait for test workload to be ready
  log_info "Waiting for test-backend to be ready"
  kubectl wait --for=condition=ready pod -l app=test-backend -n "$NAMESPACE" --timeout=300s

  log_info "Pavis infrastructure deployed successfully"
}

deploy_envoy_infrastructure() {
  log_info "Deploying Envoy xDS control plane and test workloads"

  local manifests_dir="${BENCH_ROOT}/bench/config/system/envoy"

  if [[ ! -d "$manifests_dir" ]]; then
    exit_with_error "Envoy manifests directory not found: $manifests_dir"
  fi

  # Build and load xDS server image
  log_info "Building envoy-xds-server:local image"
  local build_mode
  build_mode="$(resolve_docker_build_mode)"
  if should_quiet_build; then
    make -C "${BENCH_ROOT}" docker-build IMAGE=envoy-xds-server MODE="${build_mode}" > /dev/null 2>&1
  else
    make -C "${BENCH_ROOT}" docker-build IMAGE=envoy-xds-server MODE="${build_mode}"
  fi

  log_info "Loading envoy-xds-server image into kind cluster"
  kind load docker-image envoy-xds-server:local --name "$CLUSTER_NAME"

  # Apply xDS deployment
  kubectl apply -f "$manifests_dir/xds-deployment.yaml" -n "$NAMESPACE"

  # Wait for xDS server to be ready
  log_info "Waiting for envoy-xds to be ready"
  kubectl wait --for=condition=ready pod -l app=envoy-xds -n "$NAMESPACE" --timeout=120s

  # Apply test workload
  kubectl apply -f "$manifests_dir/test-workload.yaml" -n "$NAMESPACE"

  # Wait for test workload to be ready
  log_info "Waiting for envoy-test-backend to be ready"
  kubectl wait --for=condition=ready pod -l app=envoy-test-backend -n "$NAMESPACE" --timeout=120s

  log_info "Envoy infrastructure deployed successfully"
}

deploy_linkerd_infrastructure() {
  log_info "Deploying Linkerd control plane and test workloads"

  # Check if linkerd is already installed
  if linkerd check --pre > /dev/null 2>&1; then
    log_info "Linkerd pre-check passed"
  else
    log_warn "Linkerd pre-check failed, attempting installation anyway"
  fi

  # Install linkerd control plane
  log_info "Installing Linkerd control plane"
  linkerd install --crds | kubectl apply -f - > /dev/null 2>&1
  linkerd install | kubectl apply -f - > /dev/null 2>&1

  # Wait for linkerd control plane to be ready
  log_info "Waiting for Linkerd control plane to be ready"
  linkerd check --wait=5m > /dev/null 2>&1 || {
    log_error "Linkerd control plane failed to become ready"
    linkerd check
    return 1
  }

  log_info "Linkerd control plane ready"

  # Deploy test workload with linkerd injection
  local manifests_dir="${BENCH_ROOT}/bench/config/system/linkerd"

  if [[ ! -d "$manifests_dir" ]]; then
    exit_with_error "Linkerd manifests directory not found: $manifests_dir"
  fi

  kubectl apply -f "$manifests_dir/test-workload.yaml" -n "$NAMESPACE"

  # Wait for test workload to be ready (linkerd proxy + app)
  log_info "Waiting for linkerd-test-backend to be ready"
  kubectl wait --for=condition=ready pod -l app=linkerd-test-backend -n "$NAMESPACE" --timeout=120s

  log_info "Linkerd infrastructure deployed successfully"
}

setup_environment_system() {
  bench_print_step "Setting up system mode (Kubernetes) environment"

  check_kind_requirements
  create_kind_cluster
  allow_control_plane_scheduling_if_needed
  install_metrics_server
  build_docker_images
  load_images_to_kind
  create_namespace

  # Deploy infrastructure based on proxy type
  case "${BENCH_PROXY:-pavis}" in
    pavis)
      deploy_pavis_infrastructure
      ;;
    envoy)
      deploy_envoy_infrastructure
      ;;
    linkerd)
      deploy_linkerd_infrastructure
      ;;
    *)
      exit_with_error "Unsupported proxy for system mode: ${BENCH_PROXY}"
      ;;
  esac

  # Export cluster context for use in tests
  export BENCH_KIND_CLUSTER="$CLUSTER_NAME"
  export BENCH_NAMESPACE="$NAMESPACE"
  export BENCH_KUBECONFIG="${HOME}/.kube/config"

  persist_env_var "BENCH_KIND_CLUSTER" "$CLUSTER_NAME"
  persist_env_var "BENCH_NAMESPACE" "$NAMESPACE"

  log_info "System mode environment ready for ${BENCH_PROXY}"
}

# ============================================================================
# Standalone Mode (Docker Compose) Functions
# ============================================================================

setup_environment_standalone() {
  bench_print_step "Setting up standalone mode (Docker Compose) environment"

  # Standalone mode requires these
  : "${BENCH_PVS_CONFIG:?BENCH_PVS_CONFIG is required}"
  : "${BENCH_DOCKER_COMPOSE:?BENCH_DOCKER_COMPOSE is required}"

  if [[ ! -x "$BENCH_LOADGEN_BIN" ]]; then
    bench_print_step "Building bench-loadgen binary"
    cargo build -p pavis-benchkit --bin bench-loadgen --release
  fi

  detect_cpu_pinning

  if [[ "${BENCH_PROFILE:-}" == "github" ]]; then
    BACKEND_CPUSET=""
    PROXY_CPUSET=""
    BENCH_LOADGEN_CPUSET=""
  fi

  if [[ "${BENCH_PROFILE:-}" == "workstation" ]]; then
    if [[ "$(uname -s)" != "Linux" ]]; then
      log_warn "CPU pinning and memory limits are Linux-only; skipping workstation pinning checks"
    else
      require_cmd taskset
      if [[ -z "${BACKEND_CPUSET:-}" || -z "${PROXY_CPUSET:-}" || -z "${BENCH_LOADGEN_CPUSET:-}" ]]; then
        exit_with_error "CPU pinning is required for BENCH_PROFILE=workstation (set BACKEND_CPUSET/PROXY_CPUSET/BENCH_LOADGEN_CPUSET)"
      fi
      local backend_cpu_count
      local proxy_cpu_count
      local loadgen_cpu_count
      backend_cpu_count=$(count_cpuset "$BACKEND_CPUSET")
      proxy_cpu_count=$(count_cpuset "$PROXY_CPUSET")
      loadgen_cpu_count=$(count_cpuset "$BENCH_LOADGEN_CPUSET")
      if [[ "$backend_cpu_count" != "1" || "$proxy_cpu_count" != "2" || "$loadgen_cpu_count" != "1" ]]; then
        exit_with_error "Workstation profile requires 4 dedicated cores (1 loadgen, 1 upstream, 2 proxy)"
      fi
      if [[ -z "${CPU_LIMIT:-}" ]]; then
        proxy_cpu_count=$(count_cpuset "$PROXY_CPUSET")
        if [[ -n "$proxy_cpu_count" ]]; then
          export CPU_LIMIT="$proxy_cpu_count"
          persist_env_var "CPU_LIMIT" "$CPU_LIMIT"
        fi
      fi
      if [[ -z "${BACKEND_CPU_LIMIT:-}" ]]; then
        backend_cpu_count=$(count_cpuset "$BACKEND_CPUSET")
        if [[ -n "$backend_cpu_count" ]]; then
          export BACKEND_CPU_LIMIT="$backend_cpu_count"
          persist_env_var "BACKEND_CPU_LIMIT" "$BACKEND_CPU_LIMIT"
        fi
      fi
      if [[ -z "${MEMORY_LIMIT:-}" ]]; then
        export MEMORY_LIMIT="1G"
        persist_env_var "MEMORY_LIMIT" "$MEMORY_LIMIT"
      fi
    fi
  fi

  if [[ "$BENCH_PROXY" == "pavis" ]]; then
    ensure_pavis_config
  else
    BENCH_PVS_GENERATED=false
  fi

  export BACKEND_CPUSET
  export PROXY_CPUSET
  export BENCH_LOADGEN_CPUSET
  export BENCH_PVS_GENERATED

  persist_env_var "BACKEND_CPUSET" "${BACKEND_CPUSET:-}"
  persist_env_var "PROXY_CPUSET" "${PROXY_CPUSET:-}"
  persist_env_var "BENCH_LOADGEN_CPUSET" "${BENCH_LOADGEN_CPUSET:-}"
  persist_env_var "BENCH_PVS_GENERATED" "${BENCH_PVS_GENERATED:-false}"

  local host_info="${BENCH_OUTPUT_DIR}/${BENCH_MODE}/.host_info"
  collect_system_info > "$host_info"
  export BENCH_HOST_INFO="$host_info"
  persist_env_var "BENCH_HOST_INFO" "$host_info"
  persist_env_var "BENCH_LOADGEN_BIN" "$BENCH_LOADGEN_BIN"

  log_info "Standalone mode environment ready for ${BENCH_PROXY}"
}

count_cpuset() {
  local cpuset="$1"
  if [[ -z "$cpuset" ]]; then
    echo ""
    return
  fi
  local count
  count=$(_expand_cpuset "$cpuset" | sort -n | uniq | wc -l | tr -d ' ')
  echo "$count"
}

ensure_pavis_config() {
  BENCH_PVS_GENERATED=false
  if [[ ! -f "$BENCH_PVS_CONFIG" ]]; then
    bench_print_step "Generating .pvs config from ${BENCH_ROOT}/bench/config/standalone/pavis.yaml"
    local pavctl="${BENCH_ROOT}/target/release/pavctl"
    if [[ ! -x "$pavctl" ]]; then
      bench_print_step "Building pavctl"
      cargo build -p pavctl --release
    fi
    "$pavctl" gen "${BENCH_ROOT}/bench/config/standalone/pavis.yaml" "$BENCH_PVS_CONFIG"
    BENCH_PVS_GENERATED=true
  fi
}

# Expand cpuset string like "0-1,4,6-7" into lines: 0 1 4 6 7
_expand_cpuset() {
  local s="${1//[[:space:]]/}"
  local part a b
  IFS=',' read -r -a parts <<< "$s"
  for part in "${parts[@]}"; do
    [[ -z "$part" ]] && continue
    if [[ "$part" =~ ^([0-9]+)-([0-9]+)$ ]]; then
      a="${BASH_REMATCH[1]}"; b="${BASH_REMATCH[2]}"
      if (( a <= b )); then
        for ((i=a; i<=b; i++)); do echo "$i"; done
      else
        for ((i=a; i>=b; i--)); do echo "$i"; done
      fi
    elif [[ "$part" =~ ^[0-9]+$ ]]; then
      echo "$part"
    fi
  done
}

detect_cpu_pinning() {
  BACKEND_CPUSET="${BACKEND_CPUSET:-}"
  PROXY_CPUSET="${PROXY_CPUSET:-}"
  BENCH_LOADGEN_CPUSET="${BENCH_LOADGEN_CPUSET:-}"

  # If user explicitly set either, respect it and return
  if [[ -n "$BACKEND_CPUSET" || -n "$PROXY_CPUSET" || -n "$BENCH_LOADGEN_CPUSET" ]]; then
    return
  fi

  local cpuset_file=""
  local cpuset_effective_file=""
  if [[ -f /sys/fs/cgroup/cpuset/cpuset.cpus ]]; then
    cpuset_file=/sys/fs/cgroup/cpuset/cpuset.cpus
  elif [[ -f /sys/fs/cgroup/cpuset.cpus ]]; then
    cpuset_file=/sys/fs/cgroup/cpuset.cpus
  fi
  if [[ -f /sys/fs/cgroup/cpuset.cpus.effective ]]; then
    cpuset_effective_file=/sys/fs/cgroup/cpuset.cpus.effective
  elif [[ -f /sys/fs/cgroup/cpuset/cpuset.cpus.effective ]]; then
    cpuset_effective_file=/sys/fs/cgroup/cpuset/cpuset.cpus.effective
  fi

  # If we cannot detect, default to "workstation dev" assumption
  if [[ -z "$cpuset_file" ]]; then
    BACKEND_CPUSET=0
    PROXY_CPUSET=1-2
    return
  fi

  local available
  available="$(cat "$cpuset_file" 2>/dev/null || true)"
  available="${available//$'\n'/}"
  if [[ -z "$available" && -n "$cpuset_effective_file" ]]; then
    available="$(cat "$cpuset_effective_file" 2>/dev/null || true)"
    available="${available//$'\n'/}"
  fi
  if [[ -z "$available" && -x "$(command -v nproc)" ]]; then
    local cpu_count
    cpu_count=$(nproc 2>/dev/null || echo "")
    if [[ "$cpu_count" =~ ^[0-9]+$ && "$cpu_count" -gt 0 ]]; then
      available="0-$((cpu_count - 1))"
    fi
  fi
  if [[ -z "$available" ]]; then
    # Unknown: safest is to disable pinning rather than guess
    BACKEND_CPUSET=""
    PROXY_CPUSET=""
    return
  fi

  # If effective cpuset is narrower, prefer it for pinning.
  if [[ -n "$cpuset_effective_file" ]]; then
    local effective
    effective="$(cat "$cpuset_effective_file" 2>/dev/null || true)"
    effective="${effective//$'\n'/}"
    if [[ -n "$effective" ]]; then
      available="$effective"
    fi
  fi

  # Build sorted unique cpu list
  mapfile -t cpus < <(_expand_cpuset "$available" | sort -n | uniq)
  local n="${#cpus[@]}"

  if (( n >= 4 )); then
    # Pin: backend to first cpu, proxy to next two, loadgen to fourth.
    BACKEND_CPUSET="${cpus[0]}"
    if (( cpus[2] == cpus[1] + 1 )); then
      PROXY_CPUSET="${cpus[1]}-${cpus[2]}"
    else
      PROXY_CPUSET="${cpus[1]},${cpus[2]}"
    fi
    BENCH_LOADGEN_CPUSET="${cpus[3]}"
  elif (( n == 2 )); then
    # Pin: backend to first cpu, proxy to second
    BACKEND_CPUSET="${cpus[0]}"
    PROXY_CPUSET="${cpus[1]}"
  else
    # Not enough cores to satisfy isolation; disable pinning explicitly
    BACKEND_CPUSET=""
    PROXY_CPUSET=""
    BENCH_LOADGEN_CPUSET=""
  fi
}

# ============================================================================
# Main Setup Entry Point
# ============================================================================

setup_environment() {
  load_persisted_env
  : "${BENCH_ROOT:?BENCH_ROOT is required}"
  : "${BENCH_PROXY:?BENCH_PROXY is required}"
  : "${BENCH_LOADGEN_BIN:?BENCH_LOADGEN_BIN is required}"

  local mode="${BENCH_MODE:-}"

  # If MODE is not set, run both standalone and system
  if [[ -z "$mode" ]]; then
    log_info "MODE not set, running both standalone and system modes"

    # Run standalone mode
    export BENCH_MODE="standalone"
    setup_environment_standalone

    # Run system mode
    export BENCH_MODE="system"
    setup_environment_system

    return 0
  fi

  # Single mode execution
  case "$mode" in
    standalone)
      setup_environment_standalone
      ;;
    system)
      setup_environment_system
      ;;
    *)
      exit_with_error "Invalid BENCH_MODE: $mode (expected standalone, system, or unset for both)"
      ;;
  esac
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  setup_environment "$@"
fi

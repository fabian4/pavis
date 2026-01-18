# Benchmark Audit Summary

- **Audit Phase**: Final Summary
- **Target Module**: Benchmark
- **Generation Timestamp**: 2026-01-13T16:20:00Z
- **AI Model Identifier**: Gemini 2.0 Flash

## Phase 0: Inventory & Benchmark Topology

### Component Inventory
- **Orchestration**: `bench/run.sh`, `bench/scripts/*.sh` (setup, benchmark, cleanup, validate).
- **Traffic Generation**: `bench-loadgen` (open-loop, Rust), `wrk` (closed-loop, C).
- **Upstream / Backend**: `bench-upstream` (Rust, Hyper-based).
- **Proxy Under Test**: `pavis`, `envoy`, `nginx`, `haproxy` (Dockerized).
- **Result Aggregation**: `summarize.sh` (CSV generation), `report.sh` (Markdown visualization).

### Execution Topology
- **Infrastructure**: Docker Compose orchestration.
- **Network**: Bridge network; proxies access `bench-upstream` via alias `backend:80`.
- **CPU Pinning**: 
  - Backend: CPU 0 (`cpuset: 0`)
  - Proxies: CPUs 1-2 (`cpuset: 1-2`)
- **Control Flow**: `run.sh` triggers `setup_environment` (generates `.pvs` via `pavctl`), then executes case scripts (`bench/cases/*.sh`) which call `wrk` or `bench-loadgen`.

## Phase 1: Workload & Upstream Semantics

### Case Mapping
- **Throughput** (`throughput_short_1x`): Closed-loop (`wrk`) to find saturation ceiling.
- **Latency** (`latency_short_1x`, `latency_extended_1x`): Open-loop (`bench-loadgen`) at fixed 10k RPS.
- **Stress** (`concurrency_short_1x`, `churn_short_1x`): High-connection and high-churn scenarios using `wrk`.

### Upstream Semantics
- **Behavior**: `bench-upstream` responds with fixed-size payloads (default 64B) or simulated sleep (`/sleep?ms=X`).
- **Determinism**: Highly deterministic; uses a multi-threaded Tokio runtime with a fixed number of workers (default 2).
- **Limitations**: Only HTTP/1.1; no HTTP/2 or gRPC support. Metrics are supported via `/metrics` (Prometheus format).

## Phase 2: Fairness & Comparability

### Resource Parity
- **Limits**: All proxies are limited to 2 CPUs and 512M Memory via Docker Compose `deploy.resources`.
- **Mismatch Found**: `pavis.yaml` configures 4 workers (`workers: 4`), while `envoy.yaml`, `nginx.conf`, and `haproxy.cfg` are configured for 2 threads/processes. This gives Pavis a potential advantage in multi-core utilization or context switching efficiency under the 2-CPU limit.

### Configuration Symmetry
- **Routing**: Identical prefix-match (`/`) forwarding to `backend:80`.
- **Keepalive**: Nginx (`keepalive 1000`) and HAProxy use connection pooling, matching Pavis's default behavior.

## Phase 3: Measurement Integrity & Saturation Handling

### Saturation Detection
- **Mechanism**: `bench-loadgen` uses a non-blocking `try_acquire` on a concurrency semaphore (default 500).
- **Signal**: If the proxy cannot keep up with the inter-arrival rate, `dropped` requests are incremented and reported as a primary failure signal.
- **Error Attribution**: `bench-loadgen` distinguishes between `dropped` (saturation) and `errors` (timeouts/5xx), allowing clear attribution to proxy capacity vs stability.

## Phase 4: Determinism, Reproducibility & CI Effects

### Determinism Controls
- **Upstream**: Constant response time (no artificial jitter in default cases).
- **Loadgen**: Drift-free scheduling computes deadlines relative to test start time.

### CI Risks
- **CPU Contention**: Standard GitHub Actions runners (2 vCPUs) cannot satisfy the `cpuset` pinning requirements (Cores 0, 1, and 2 requested).
- **Evidence**: `bench/docker-compose.yaml` pins to 3 distinct cores, which will cause "resource busy" or "no core" errors/contention on 2-core CI runners, potentially invalidating CI performance results.

## Phase 5: Result Interpretation & Claim Validity

### Claim Traceability
- **Usable Capacity**: Correctly derived in `report.sh` as `max_rps / latency_rps`, identifying the "knee" in the performance curve.
- **Tail Latency**: `p99_ms` and `p99_iqr` (Inter-Quartile Range) provide a high-signal measure of stability over extended runs.

### Overinterpretation Risk
- **Synthetic Backend**: `bench-upstream` is extremely fast. Proxy performance differences may be amplified compared to real-world scenarios where backend latency dominates the total response time.

## FINAL SUMMARY: Executive Verdict

### 1. Verdict
**Benchmark Methodology is Partially Sound (Caveats Exist)**

### 2. Top Risks
1. **Thread/Worker Mismatch (Phase 2)**: Pavis is configured with 4 workers vs 2 for competitors, under a 2-CPU limit. This skews efficiency comparisons.
2. **CI Resource Starvation (Phase 4)**: The `cpuset` pinning (Cores 0-2) exceeds the capacity of standard 2-core CI runners, leading to inevitable CPU contention between the backend and proxy.
3. **PVS/YAML Discrepancy (Phase 0)**: `bench/config/standalone/pavis.yaml` specifies 4 workers, but the runtime uses a `.pvs` file generated during setup. Verification is needed to ensure the generated `.pvs` matches the audited `.yaml`.

### 3. Confidence Assessment
- **Comparability**: Medium. The worker count mismatch must be reconciled.
- **Upstream Surrogate**: High. `bench-upstream` is lightweight and deterministic.
- **Reproducibility**: High (locally), Low (in 2-core CI environments).

### 4. Next Steps
- **Align Worker Counts**: Update `pavis.yaml` to use 2 workers to match Envoy/Nginx/HAProxy baseline.
- **CI Pinning Adjustment**: Modify `docker-compose.yaml` to allow flexible pinning or disable pinning on 2-core runners to avoid contention.
- **Audit Gen Pipeline**: Verify that `pavctl gen` correctly translates `pavis.yaml` to `pavis.pvs` without altering worker settings.

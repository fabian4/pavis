# Benchmark Case Design & Methodology

This document details the test cases, environment setup, and methodological principles used in the Pavis Benchmark Suite.

## 1. Methodology Principles

The benchmark suite adheres to strict principles to ensure credibility and reproducibility:

- **Open-Loop Latency Testing**: We use `wrk2` (via `bench-loadgen`) for latency tests to avoid "Coordinated Omission," ensuring we measure true tail latency under sustained load.
- **Backend Isolation**: A specialized, deterministic backend (`bench-upstream`) is used to eliminate application-layer bottlenecks.
- **Resource Isolation**: CPU pinning separates the proxy, backend, and load generator to prevent resource contention.
- **Statistical Validation**: Critical tests support multi-run execution (N=5) with Median/IQR aggregation to filter out noise.

---

## 2. Test Environment

### System Architecture
```
┌──────────────────┐      ┌─────────────────────┐      ┌──────────────────┐
│ wrk/bench-loadgen│ ───▶ │  Proxy (container)  │ ───▶ │ bench-upstream   │
│     (host)       │      │  CPU: 1-2           │      │ CPU: 0           │
│   4 threads      │      │  cgroup-limited     │      │ deterministic    │
└──────────────────┘      └─────────────────────┘      └──────────────────┘
                            ↓                              ↓
                       Pinned CPUs                   Pinned CPU
                       (isolation)                   (isolation)
```

### Resource Configuration

- **Backend (`bench-upstream`)**:
  - Pinned to **CPU 0**.
  - No specific memory limit (lightweight Rust binary).
- **Proxy (Pavis/Envoy/Nginx/HAProxy)**:
  - Pinned to **CPUs 1-2** (2 cores).
  - Memory Limit: **512 MiB** (default baseline profile).
- **Load Generator**:
  - Runs on the host system.
  - Should ideally use CPUs other than 0-2 (not strictly enforced, but recommended).

### Deterministic Backend (`bench-upstream`)
To isolate proxy performance, we use a minimal, high-performance backend:
- **Source**: `crates/pavis-benchkit/src/bin/bench-upstream.rs`
- **Behavior**: Returns pre-allocated fixed payloads. No dynamic allocation, logging, or complex logic.
- **Endpoints**:
  - `/healthz`: Health check.
  - `/fixed`: Returns a fixed 64-byte payload.
  - `/sleep?ms=N`: Returns payload after N ms delay.

---

## 3. Load Generation Strategy

### Tools Used
1. **`wrk` (Closed-Loop)**:
   - Used for **Throughput** tests.
   - Pushes requests as fast as the server can respond.
   - Measures maximum capacity.
2. **`bench-loadgen` / `wrk2` (Open-Loop)**:
   - Used for **Latency** tests.
   - Sends requests at a fixed target rate (RPS), independent of server response time.
   - Measures latency distribution (P50, P99, P99.9) under specific load.

---

## 4. Test Case Matrix

The suite includes 6 standard test cases designed to exercise different aspects of the proxy.

| Case Name | Tool | Load Type | Duration | Connections | Description |
|-----------|------|-----------|----------|-------------|-------------|
| **`throughput_short_1x`** | `wrk` | Closed-loop | 30s | 100 | **Max Throughput.** Measures the maximum RPS the proxy can handle with a small number of persistent connections. |
| **`latency_short_1x`** | `loadgen` | Open-loop | 30s | 500 | **Baseline Latency.** Measures P99 latency at a sustained load (default 10k RPS). |
| **`latency_extended_1x`** | `loadgen` | Open-loop | 300s | 500 | **Tail Stability.** Longer duration to detect jitter, GC pauses, or resource leaks over time. |
| **`concurrency_short_1x`** | `wrk` | Closed-loop | 30s | 5,000 | **High Concurrency.** Tests performance with a large number of idle/active connections. |
| **`churn_short_1x`** | `wrk` | Closed-loop | 30s | 100 | **Connection Churn.** Disables keepalive (`Connection: close`). Measures handshake/teardown overhead. |
| **`reload_short_1x`** | `loadgen` | Open-loop | 30s | 500 | **Config Reload.** Measures impact on latency during hot configuration reloads (Pavis specific). |

### Workload Intensity
- **Target RPS**: 10,000 RPS (default for latency tests).
- **Payload Size**: 64 bytes (fixed).
- **Threads**: 4 threads used by load generators.

---

## 5. Metrics & Analysis

For each run, the following metrics are collected:

- **`achieved_rps`**: Requests Per Second processing rate.
- **`p99_ms`**: 99th percentile latency (milliseconds). Critical for SLAs.
- **`errors`**: Connection or socket errors (must be 0 for valid run).
- **`cpu_usage`**: Normalized CPU usage of the proxy container.
- **`memory_peak`**: Peak Resident Set Size (RSS) memory usage.

### Saturation Detection
We monitor the **Backend CPU**. If `backend_cpu > 80%`, the results are flagged as **Backend Saturated**, meaning the bottleneck is the backend, not the proxy.

---

## 6. Limitations & Known Issues

1. **macOS**: CPU pinning (`cpuset`) is not supported on Docker Desktop for Mac. Results on macOS are useful for functional testing but not for strict performance comparison.
2. **Host Noise**: Running the load generator on the same host as the docker containers can introduce context-switching noise. For production-grade benchmarking, use a separate load generator machine.
3. **HTTP/1.1 Only**: Current benchmarks cover HTTP/1.1. HTTP/2 and gRPC are planned for future updates.

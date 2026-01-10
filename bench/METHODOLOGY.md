# Pavis Benchmark Methodology

This document explains the methodological foundations
 of the Pavis benchmark suite and the design decisions made to ensure credible, reproducible, and defensible performance comparisons.

---

## Table of Contents

1. [Design Principles](#design-principles)
2. [Load Generation Strategy](#load-generation-strategy)
3. [Backend Bottleneck Elimination](#backend-bottleneck-elimination)
4. [Resource Isolation & Noise Reduction](#resource-isolation--noise-reduction)
5. [Fairness & Configuration Parity](#fairness--configuration-parity)
6. [Statistical Validity](#statistical-validity)
7. [Metrics & Observability](#metrics--observability)
8. [Workload Semantics](#workload-semantics)
9. [Limitations & Known Issues](#limitations--known-issues)
10. [References](#references)

---

## 1. Design Principles

The benchmark suite is designed around the following principles:

### Credibility
- **Open-loop latency testing**: Latency benchmarks use wrk2 with fixed target RPS to avoid coordinated omission
- **Multi-run validation**: Statistical aggregation (median, IQR) across N=5 runs for critical tests
- **Backend isolation**: Minimal backend option eliminates application-layer bottlenecks

### Reproducibility
- **Fixed configurations**: All proxy configs documented with semantic equivalence
- **CPU pinning**: Distinct CPU sets for proxy, backend, and load generator
- **Controlled resources**: cgroup limits for CPU and memory

### Fairness
- **Configuration parity**: All proxies configured with equivalent semantics (keepalive, HTTP/1.1, etc.)
- **Explicit documentation**: Fairness checklist maps each proxy's config to common behavior
- **Backend transparency**: Backend type and saturation status reported in results

### Explainability
- **Workload classification**: Clear labeling of open-loop vs closed-loop, latency vs throughput
- **Diagnostic metrics**: Backend CPU, proxy CPU, memory, error rates
- **Confidence indicators**: Multi-run IQR shows result stability

---

## 2. Load Generation Strategy

### 2.1 Open-Loop vs Closed-Loop

**Open-Loop (wrk2)**
- **Used for**: Latency workloads
- **Rationale**: Prevents coordinated omission by maintaining constant request rate independent of response time
- **Target RPS**: Specified per workload (e.g., 10k RPS for baseline latency test)
- **Metrics**: Achieved RPS, P50/P90/P99/P99.9 latency, errors

**Closed-Loop (wrk)**
- **Used for**: Throughput, concurrency, churn workloads
- **Rationale**: Measures maximum achievable throughput under connection limits
- **Metrics**: Achieved RPS, average latency, P99 latency, errors

### 2.2 Workload Matrix

| Workload       | Connections | Load Type   | Target RPS  | Purpose                                  |
|----------------|-------------|-------------|-------------|------------------------------------------|
| `throughput`   | 100         | Closed-loop | -           | Maximum RPS under light load             |
| `latency`      | 500         | Open-loop   | 10,000      | Tail latency under sustained load        |
| `concurrency`  | 5,000       | Closed-loop | -           | High connection count stress             |
| `churn`        | 100         | Closed-loop | -           | Rapid connect/disconnect handshake cost  |
| `reload`       | 500         | Open-loop   | 5,000       | Hot-reload latency jitter (Pavis-specific) |

### 2.3 Load Generator Configuration

- **Threads**: 4 (default)
- **Warmup**: 5 seconds (excluded from measurements)
- **Duration**: 30s (short), 300s (extended)
- **HTTP**: HTTP/1.1, keepalive enabled (except churn)

---

## 3. Backend Bottleneck Elimination

### 3.1 Problem Statement

Using httpbin (or similar application backends) can introduce hidden bottlenecks:
- Python/application-level processing overhead
- JSON serialization/parsing
- Variable response times
- Backend saturation under high load

### 3.2 Solution: Dual Backend Strategy

**Backend Option 1: httpbin (Functional Realism)**
- **Use case**: Tests requiring realistic application behavior
- **Pros**: Functional HTTP responses, realistic latency distribution
- **Cons**: May saturate under high RPS, adds non-proxy overhead
- **Resource limits**: 2 CPU cores, 1GB memory

**Backend Option 2: Minimal (Dataplane Isolation)**
- **Use case**: Tests focused on proxy dataplane performance
- **Pros**: Fixed 200 response, minimal overhead, deterministic
- **Cons**: Not representative of real-world applications
- **Resource limits**: 2 CPU cores, 512MB memory
- **Implementation**: Lightweight Go server (39 bytes JSON response)

### 3.3 Backend Selection Guidelines

| Test Type                | Recommended Backend |
|--------------------------|---------------------|
| Throughput (short)       | httpbin             |
| Latency (short)          | httpbin             |
| Throughput (extended)    | minimal             |
| Latency (extended)       | minimal             |
| Pavis-specific           | minimal             |

### 3.4 Backend Saturation Detection

All reports include `backend_cpu_pct` and `backend_saturated` flag (CPU > 80%).
If backend is saturated, results may reflect backend limits rather than proxy performance.

---

## 4. Resource Isolation & Noise Reduction

### 4.1 CPU Pinning Strategy

**Objective**: Prevent interference between load generator, proxy, and backend.

**CPU Allocation**:
- **Backend**: CPU 0 (cpuset_cpus: "0")
- **Proxy**: CPUs 1-2 (cpuset_cpus: "1-2")
  - For CPU-limited tests (1 core): cpuset_cpus: "1"
- **Load Generator**: Runs on host, should use distinct cores if possible (not enforced)

**Rationale**: Separating backend from proxy eliminates shared-core contention. Load generator on host allows full wrk/wrk2 capabilities.

### 4.2 Container Resource Limits

**cgroup Limits**:
- CPU: `--cpus` (e.g., 1.0, 2.0)
- Memory: `--memory` (e.g., 256M, 512M, 1G)

**Profiles**:
- `baseline`: 2 CPUs, 512 MiB
- `cpu-limited`: 1 CPU, 512 MiB
- `memory-limited`: 2 CPUs, 256 MiB

### 4.3 CPU Governor

**Assumption**: Host CPU governor set to `performance` for stable clock speeds.

**Check**:
```bash
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

**Set (if needed)**:
```bash
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

**Note**: Not enforced by benchmark suite. Documented as best-effort recommendation.

---

## 5. Fairness & Configuration Parity

### 5.1 Fairness Checklist

All proxies configured with equivalent semantics to ensure fair comparison:

| Semantic Behavior              | Pavis                          | Envoy                                | Nginx                                     | HAProxy                          |
|--------------------------------|--------------------------------|--------------------------------------|-------------------------------------------|----------------------------------|
| **Workers/Threads**            | Runtime-detected (2 expected)  | `--concurrency 2`                    | `worker_processes 2`                      | `nbthread 2`                     |
| **Downstream Keepalive**       | Enabled (default)              | Enabled (default)                    | `keepalive_timeout 65; keepalive_requests 10000` | Enabled (HTTP mode)              |
| **Upstream Keepalive**         | Connection pool (default)      | Connection pool (cluster config)     | `keepalive 100` (upstream block)          | Enabled (default)                |
| **HTTP Version**               | HTTP/1.1                       | HTTP/1.1                             | `proxy_http_version 1.1`                  | HTTP/1.1 (default in HTTP mode)  |
| **Idle Timeout (client)**      | 30s (default)                  | 3600s (route timeout, overridden)    | `keepalive_timeout 65`                    | `timeout client 30s`             |
| **Idle Timeout (upstream)**    | 30s (default)                  | Connection pool idle timeout         | Persistent (with keepalive)               | `timeout server 30s`             |
| **Max Connections (downstream)** | No explicit limit            | No explicit limit                    | `worker_connections 10000` (per worker)   | `maxconn 20000` (global)         |
| **Max Connections (upstream)** | Connection pool (runtime)      | Circuit breaker config (if enabled)  | `keepalive 100` (persistent pool size)    | No explicit limit                |
| **Logging**                    | Disabled for benchmark         | `/dev/null`                          | `access_log off; error_log /dev/null`     | `log /dev/null`                  |

### 5.2 Configuration Files

See `bench/config/` for full proxy configurations:
- `pavis.yaml`
- `envoy.yaml`
- `nginx.conf`
- `haproxy.cfg`

---

## 6. Statistical Validity

### 6.1 Multi-Run Testing

**Default**: Single run (N=1)
**Statistical validation**: N=5 runs for critical tests

**Tests with multi-run (N=5)**:
- `latency_baseline_extended_1x` (300s, 5 iterations)
- `reload_baseline_short_1x` (Pavis hot-reload test)

### 6.2 Aggregation Methods

**Primary Metric**: Median (robust to outliers)
**Variability**: IQR (interquartile range = Q3 - Q1)

**Reported Metrics**:
- `rps_median`: Median RPS across runs
- `rps_iqr`: IQR of RPS (indicates stability)
- `p99_median`: Median P99 latency
- `p99_iqr`: IQR of P99 latency

**Interpretation**:
- Low IQR: Stable, repeatable performance
- High IQR: High variance, investigate noise sources

### 6.3 Run Order Randomization

**Current Implementation**: Sequential runs (not randomized)
**Future Enhancement**: Randomize order to avoid warm-cache bias
**Workaround**: 5s cooldown between runs to flush caches

---

## 7. Metrics & Observability

### 7.1 Primary Comparison Metrics

Used for proxy-to-proxy performance comparison:

| Metric          | Unit       | Description                                    |
|-----------------|------------|------------------------------------------------|
| `achieved_rps`  | req/s      | Achieved requests per second                   |
| `p50_ms`        | ms         | 50th percentile latency                        |
| `p90_ms`        | ms         | 90th percentile latency                        |
| `p99_ms`        | ms         | 99th percentile latency                        |
| `p999_ms`       | ms         | 99.9th percentile latency (wrk2 only)          |
| `avg_cpu_pct`   | %          | Average proxy CPU usage (normalized to vCPU)   |
| `peak_mem_mib`  | MiB        | Peak proxy RSS memory                          |
| `errors`        | count      | Socket errors (connect, read, write, timeout)  |

### 7.2 Diagnostic Metrics

Used to explain or validate primary metrics:

| Metric               | Unit       | Description                                    |
|----------------------|------------|------------------------------------------------|
| `target_rps`         | req/s      | Target RPS (open-loop only)                    |
| `load_type`          | string     | `open-loop` or `closed-loop`                   |
| `backend_type`       | string     | `httpbin` or `minimal`                         |
| `backend_cpu_pct`    | %          | Backend CPU usage (avg)                        |
| `backend_saturated`  | bool       | `true` if backend CPU > 80%                    |
| `run_count`          | count      | Number of iterations for this config           |
| `rps_median`         | req/s      | Median RPS (multi-run)                         |
| `rps_iqr`            | req/s      | RPS interquartile range                        |
| `p99_median`         | ms         | Median P99 latency (multi-run)                 |
| `p99_iqr`            | ms         | P99 interquartile range                        |

---

## 8. Workload Semantics

### 8.1 Throughput Workload

**Type**: Closed-loop
**Connections**: 100
**Expected Behavior**: Proxy should serve maximum RPS under light load
**Saturation Point**: Not expected to saturate (baseline resource profile)

**Metrics of Interest**:
- RPS (higher is better)
- P99 latency (should remain low)
- CPU efficiency (RPS per CPU %)

### 8.2 Latency Workload

**Type**: Open-loop
**Connections**: 500
**Target RPS**: 10,000 (baseline), 20,000 (2x intensity)
**Expected Behavior**: Measure tail latency under sustained load
**Saturation Point**: If achieved RPS < target RPS, proxy is saturated

**Metrics of Interest**:
- P99, P99.9 latency (lower is better)
- Achieved vs target RPS (should match)
- Error rate (should be zero)

### 8.3 Concurrency Workload

**Type**: Closed-loop
**Connections**: 5,000 (baseline), 10,000 (2x intensity)
**Expected Behavior**: Stress proxy with many concurrent idle connections
**Distinction**: Not just connection count, but connection + request stress

**Metrics of Interest**:
- RPS under high connection count
- Memory usage (scales with connections)
- P99 latency (may degrade under high connection count)

### 8.4 Churn Workload

**Type**: Closed-loop
**Connections**: 100
**Connection Behavior**: `Connection: close` header (no keepalive)
**Expected Behavior**: Rapidly open/close connections to measure handshake cost
**Target**: New connections per second (not just requests)

**Metrics of Interest**:
- RPS (reflects handshake rate)
- CPU overhead of connection setup/teardown
- Memory churn

### 8.5 Reload Workload (Pavis-Specific)

**Type**: Open-loop
**Connections**: 500
**Target RPS**: 5,000
**Special Behavior**: Trigger config reload every 10 seconds during benchmark
**Expected Behavior**: Pavis should maintain stable latency during reload (frozen dataplane)

**Metrics of Interest**:
- P99 latency spikes during reload
- Error rate during reload
- Multi-run IQR (should be low for Pavis)

**Implementation Note**: Reload triggering mechanism not yet implemented. Currently runs as standard latency test.

---

## 9. Limitations & Known Issues

### 9.1 Known Limitations

1. **Load Generator CPU Pinning**: wrk/wrk2 runs on host, not pinned to specific CPUs. May interfere with proxy/backend if host has limited cores.

2. **Reload Benchmark**: Reload triggering mechanism not implemented. Placeholder test runs as standard open-loop latency test.

3. **Target RPS Extraction**: wrk2 target RPS not explicitly captured in output parsing (requires command-line inspection or config correlation).

4. **CPU Governor**: Not enforced by benchmark suite. Users should manually set to `performance` for stable results.

5. **Single-host Testing**: All containers run on same host. Multi-node distributed testing not supported.

6. **HTTP/2 & gRPC**: Current tests focus on HTTP/1.1. HTTP/2 and gRPC benchmarks not included.

### 9.2 Validity Threats

**Threats to Internal Validity**:
- Warm cache effects (mitigated by warmup phase, cooldown between runs)
- Container scheduler noise (mitigated by CPU pinning)
- Backend saturation (mitigated by backend selection, saturation detection)

**Threats to External Validity**:
- Synthetic workloads may not represent real-world traffic patterns
- httpbin backend not representative of production applications
- Single-host setup may not capture distributed system effects

**Threats to Construct Validity**:
- Closed-loop latency testing subject to coordinated omission (mitigated by open-loop wrk2 for latency tests)
- Single-run results may not capture variance (mitigated by multi-run for critical tests)

---

## 11. Metrics & Interpretation

### Load Types
- **open-loop**: wrk2 with fixed target RPS → best for latency measurement.
- **closed-loop**: wrk maximizing throughput → best for RPS measurement.

### Backend Types
- **httpbin**: Python application (realistic but may saturate).
- **minimal**: Go server (eliminates backend bottleneck).

### Key Metrics

| Metric | Good Value | Meaning |
|--------|-----------|---------|
| `achieved_rps` | High | Requests per second |
| `p99_ms` | <10ms | 99th percentile latency |
| `p999_ms` | <50ms | 99.9th percentile latency |
| `errors` | 0 | Socket errors |
| `backend_saturated` | false | Backend not bottleneck |
| `rps_iqr` | Low | Stable performance across runs |

---

## 12. Benchmark Matrix Detail

The full matrix consists of **46 total runs** = (11 configurations × 4 proxies) + 2 Pavis-specific tests.

### dimensions

| Dimension | Values | Description |
|-----------|--------|-------------|
| **Workload** | throughput, latency, concurrency, churn, reload | Operational pattern |
| **Resource** | baseline, cpu-limited, memory-limited | Container cgroup limits |
| **Duration** | short (30s), extended (300s) | Measurement window |
| **Intensity** | 1x, 2x | Connection count multiplier |
| **Backend** | httpbin, minimal | Backend service type |
| **Runs** | single (N=1), multi (N=5) | Statistical validation |

### Workload Matrix

| Workload | Connections | Load Type | Target RPS | Description |
|----------|:-----------:|-----------|:----------:|-------------|
| throughput | 100 | closed-loop | - | RPS under light load |
| latency | 500 | **open-loop** | 10,000 | Tail latency under sustained load |
| concurrency | 5,000 | closed-loop | - | High concurrent connection stress |
| churn | 100 | closed-loop | - | Rapid connect/disconnect handshake cost |
| reload | 500 | **open-loop** | 5,000 | Hot-reload latency jitter (Pavis specific) |

---

## 13. References

### Load Testing Methodology

- Gil Tene, "How NOT to Measure Latency" (2015)
  https://www.youtube.com/watch?v=lJ8ydIuPFeU

- Coordinated Omission Problem
  https://github.com/HdrHistogram/HdrHistogram/wiki/Coordinated-Omission

- wrk2: A constant throughput, correct latency recording variant of wrk
  https://github.com/giltene/wrk2

### Statistical Methods

- Robust Statistics: Median and IQR
  https://en.wikipedia.org/wiki/Interquartile_range

### CPU Isolation

- Docker cpuset_cpus documentation
  https://docs.docker.com/config/containers/resource_constraints/#configure-the-default-cfs-scheduler

---

**End of Methodology Document**

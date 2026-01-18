# Pavis Benchmarking Guide

This document provides a comprehensive guide to Pavis benchmarking methodology, test cases, and fairness criteria.

---

## Table of Contents

1. [Methodology](#1-methodology)
2. [Standalone Dataplane Cases](#2-standalone-dataplane-cases)
3. [System / Kubernetes Cases](#3-system--kubernetes-cases)
4. [Fairness & Configuration Equivalence](#4-fairness--configuration-equivalence)

---

## 1. Methodology

### 1.1. Introduction

Naive benchmarking of network proxies focuses almost exclusively on "max throughput" (Requests Per Second) under ideal conditions. While useful for marketing, this metric is insufficient for engineering robust service mesh infrastructure. A proxy that pushes 100k RPS but imposes 500ms tail latency during garbage collection, or consumes 4GB of RAM to handle a traffic spike, is operationally unfit for production.

This document establishes a rigorous, multidimensional framework for evaluating **production-grade service mesh sidecars**. It distinguishes between *kernel development benchmarks* (micro-benchmarks of packet forwarding) and *productization benchmarks* (macro-benchmarks of full protocol stacks).

Our goal is not merely to measure speed, but to quantify predictability, efficiency, and safety. All future performance evaluations must adhere to the dimensions and constraints defined herein.

---

### 1.2. Benchmark Execution Modes

To ensure scientific rigor and clarity of purpose, all Pavis benchmarks are executed in one of two distinct modes.

#### 1.2.1. Standalone Dataplane Mode
*   **Purpose:** Measure the **intrinsic performance** of the data plane in isolation.
*   **Environment:** Minimal Docker or bare-metal environment.
*   **Constraints:** Static configuration only; no control-plane components (Relay, Config Agent) are present.
*   **Target Dimensions:** Capacity (#1), Tail Latency (#2), Stability (#3), Resource Efficiency (#4), Stress Behavior (#5), and Consistency (#7).
*   **Comparability:** Primary mode for benchmarking Pavis against industry-standard proxies (Envoy, Nginx).

#### 1.2.2. System / Kubernetes Mode
*   **Purpose:** Measure **control-plane assisted lifecycle behavior** and system-wide reliability.
*   **Environment:** Kubernetes (kind) cluster.
*   **Constraints:** Includes full system components (Relay, Agent); configuration is dynamic and pushed during tests.
*   **Target Dimensions:** Operational Characteristics (#6), Recovery (#B), and Durability (#Gate).
*   **Comparability:** Architecture-specific; measures the maturity of the Pavis ecosystem rather than micro-performance.

#### 1.2.3. Execution Profiles & Authority
Benchmark execution is further constrained by environment profile.

*   **github (CI-only):** Pavis-only regression signal; skips `latency_extended_1x`. Reports are generated from `summary.csv` and are **non-authoritative** due to shared runner variance.
*   **workstation (authoritative):** Dedicated hardware required. CPU pinning is mandatory with a 4-core allocation (1 loadgen/wrk, 1 upstream, 2 proxy) and a 1GiB proxy memory limit. Standalone payload matrix runs `throughput_short_1x`, `latency_short_1x`, and `latency_extended_1x` at `64B` and `4KiB`.

---

### 1.3. Core Evaluation Dimensions

The following seven dimensions constitute the primary axes of evaluation. Every comprehensive benchmark suite must address these dimensions to provide a complete performance profile. The dimensions are invariant across execution modes.

#### 1.3.1. Performance Ceiling (Capacity)
*   **The Question:** At what point does the system fail to process requests successfully?
*   **Production Relevance:** Determines the absolute maximum capacity of a standard unit of infrastructure (e.g., 1 CPU core), guiding capacity planning and autoscaling triggers.
*   **Primary Metrics:**
    *   **Max Sustainable RPS:** The highest load where success rate is >99.9% and P99 latency remains within defined SLOs.
    *   **Saturation Point:** The load at which CPU reaches 100% utilization.
*   **Typical Cases:** Throughput saturation tests with minimal payload logic.

#### 1.3.2. Tail Latency Quality
*   **The Question:** What is the worst-case experience for a user request?
*   **Production Relevance:** In microservices architectures, tail latencies compound across call chains. A "fast average" with a "slow tail" results in unacceptable system-wide performance.
*   **Primary Metrics:**
    *   **P99 and P99.9 Latency:** Measured in milliseconds.
    *   **Max Latency:** The absolute worst-case request time during the sample window.
*   **Typical Cases:** Fixed-rate load testing (below saturation) measuring distribution curves. Note: Averages (mean) are explicitly discarded as a primary metric.

#### 1.3.3. Stability & Variance
*   **The Question:** Is performance predictable over time?
*   **Production Relevance:** Jitter makes autoscaling algorithms unstable and causes "thundering herd" issues in upstream services. A boring proxy is a good proxy.
*   **Primary Metrics:**
    *   **Latency Standard Deviation:** Deviation from the mean.
    *   **Coefficient of Variation (CV):** Ratio of standard deviation to the mean.
*   **Clarification:** Latency distributions in networked systems are non-normal. While mean-based statistics are provided for legacy baseline comparison, percentile-based dispersion (e.g., P99 Interquartile Range) is more representative of real-world variance and is preferred in implementation analysis.
*   **Typical Cases:** Steady-state load testing over medium duration (e.g., 10 minutes) analyzing time-series consistency.

#### 1.3.4. Resource Efficiency
*   **The Question:** What is the infrastructure cost per unit of work?
*   **Production Relevance:** Directly dictates the Total Cost of Ownership (TCO) of the service mesh.
*   **Primary Metrics:**
    *   **CPU/RPS Ratio:** Millicores consumed per 1,000 requests.
    *   **Memory Footprint:** RSS memory usage at specific load tiers.
*   **Typical Cases:** Ramping load tests correlating system telemetry with application throughput.

#### 1.3.5. Stress Behavior (Under Load)
*   **The Question:** How does the system degrade when pushed beyond its limit?
*   **Production Relevance:** Systems must degrade gracefully. Hard crashes, hanging connections, or memory exhaustion (OOM) during traffic spikes cause cascading failures.
*   **Primary Metrics:**
    *   **Goodput vs. Offered Load:** Does the system shed load or queue it indefinitely?
    *   **Error Distribution:** Immediate fast-fail (503) vs. connection timeouts.
*   **Typical Cases:** Overload tests (150% of capacity) and "spike" tests.

#### 1.3.6. Operational Characteristics
*   **The Question:** What is the impact of control-plane operations on data-plane traffic?
*   **Production Relevance:** Dynamic environments constantly push configuration updates. These updates must not cause packet drops or latency spikes.
*   **Primary Metrics:**
    *   **Reload Latency Impact:** Delta in P99 latency during configuration reload.
    *   **Convergence Time:** Time from config push to traffic shifting.
*   **Typical Cases:** Hot-restart tests, certificate rotation simulation, and dynamic upstream reconfiguration during load.

#### 1.3.7. Cross-Scenario Consistency
*   **The Question:** Does the system perform reliably across different traffic patterns?
*   **Production Relevance:** Ensures the proxy is general-purpose and not over-optimized for a single synthetic use case (e.g., 64-byte payloads).
*   **Primary Metrics:**
    *   **Performance Delta:** Variance in throughput/latency between small (1KB) and large (1MB) payloads.
    *   **Protocol Overhead:** degradation moving from HTTP/1.1 to HTTP/2 or gRPC.
*   **Typical Cases:** Matrix testing across payload sizes, connection keep-alive settings, and protocol versions.

---

### 1.4. Derived Dimensions

Derived dimensions are not independent tests; they are specific analytical cuts of data gathered during the Core dimension tests.

#### Derived Dimension A: Feature Tax
**Feature Tax** is the quantified performance cost of enabling production-essential features compared to a "naked" baseline.

*   **Definition:** `(Performance_Baseline - Performance_Feature) / Performance_Baseline`
*   **Mandatory Analysis Areas:**
    *   **TLS Tax:** The cryptographic overhead of HTTPS versus plain HTTP. This comparison may include standard HTTPS or be extended to mutual TLS (mTLS) to evaluate the additional handshake and certificate validation costs.
    *   **Observability Tax:** The CPU/Latency cost of generating high-cardinality metrics and distributed tracing spans.
*   **Usage:** Feature Tax should be applied to *Performance Ceiling* and *Resource Efficiency* analysis to set realistic production expectations.

#### Derived Dimension B: Recovery & Hysteresis
**Recovery** measures the system's ability to return to baseline state *after* a stress event has concluded.

*   **Definition:** Time-series analysis of the cool-down period following a Stress Behavior test.
*   **Mandatory Analysis Areas:**
    *   **Memory Hysteresis:** Does RSS return to baseline, or does fragmentation keep memory usage high?
    *   **Latency Normalization:** How many seconds until P99 returns to steady-state levels after the load spike ends?
*   **Relevance:** Critical for long-running processes where temporary spikes must not permanently degrade the runtime environment.

---

### 1.5. Release Gate Dimension

This dimension is computationally expensive and is reserved for major release candidates or publication checkpoints, rather than daily CI runs.

#### Durability / Soak Testing
*   **The Question:** Does the system degrade over extended periods of operation?
*   **Production Relevance:** Detects slow memory leaks, resource handle exhaustion (file descriptors), and integer overflow issues that micro-benchmarks miss.
*   **Scope:** Multi-hour (e.g., 8+ hours) continuous load runs.
*   **Pass Criteria:**
    *   **Zero Monotonic Growth:** Memory usage must plateau and not show linear growth trend.
    *   **Stable Latency:** P99 at the end of the window must be within statistically significant range of P99 at the start.
    *   **Resource Reclamation:** File descriptors must verify full closure cycles.

---

### 1.6. Methodology Guardrails

To ensure data integrity and comparability, the following practices are strictly enforced:

1.  **No Apple-to-Orange Aggregation:** Never average metrics across dissimilar test cases (e.g., do not average "1KB Payload" results with "100KB Payload" results into a single score).
2.  **Separate Open vs. Closed Loop:** Clearly distinguish between closed-loop (waiting for response) and open-loop (fire-and-forget) load generation. Closed-loop testing hides latency degradation (Coordinated Omission); Open-loop is required for accurate tail latency measurement.
3.  **Data Provenance:** Every graph must be backed by accessible tabular data. Visualizations without raw numbers are rejected.
4.  **Isolation:** Benchmarks must run on dedicated, isolated hardware/nodes. No noisy neighbors.
5.  **Reproducibility:** All configuration (OS tuning, proxy config, load generator command) must be committed as code alongside results.

---

### 1.7. Intended Outcomes

By adhering to this framework, we ensure:
*   **Accountability:** Architecture decisions are backed by data, not intuition.
*   **Standardization:** A shared language for discussing performance across engineering teams.
*   **Integrity:** Prevention of "benchmark gaming" or cherry-picking favorable scenarios.

This methodology defines the standard of quality required for the project to be considered production-ready.

---

## 2. Standalone Dataplane Cases

### 2.1. Introduction

This section operationalizes the evaluation framework for **Standalone Dataplane Mode**. 

Standalone mode is designed to measure the **intrinsic performance properties** of the Pavis data plane in isolation. It focuses on the fundamental cost of packet processing, routing, and transport management without interference or assistance from control-plane components.

### 2.2. Mode Boundary Rules

To ensure measurement integrity, the following rules are strictly enforced for all standalone benchmarks:

*   **Static Configuration:** The proxy under test MUST be initialized with a fixed configuration. No configuration updates are permitted during the measurement window.
*   **Isolation:** The environment must be isolated to prevent interference from non-dataplane processes.
*   **No External Dependencies:** Standalone benchmarks must not depend on external configuration distribution or discovery services during execution.

### 2.3. Execution Profiles

Standalone mode supports two profiles:

*   **github:** CI-only regression signal for Pavis; skips `latency_extended_1x` and produces reports derived from `summary.csv` only.
*   **workstation:** Authoritative runs on dedicated hardware with strict CPU pinning and resource limits.

### 2.4. Dimension-Indexed Benchmark Coverage

#### 2.4.1. Performance Ceiling (Capacity)

**The Question:** At what point does the system fail to process requests successfully?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `throughput_short_1x` | **Primary** | `achieved_rps` | Measures absolute max packet forwarding rate (saturation) under static configuration. |
| `concurrency_short_1x` | Secondary | `achieved_rps` | Verifies throughput degradation when forced to manage high connection counts. |

#### 2.4.2. Tail Latency Quality

**The Question:** What is the worst-case experience for a user request?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `latency_short_1x` | **Primary** | `p99_ms`, `p99.9_ms` | Standard baseline at sustainable load (default 10k RPS). |
| `latency_extended_1x` | Secondary | `p99_ms` | **Authoritative:** uses percentile-based metrics to identify tail latency trends; `max_ms` is diagnostic only. |

#### 2.4.3. Stability & Variance

**The Question:** Is performance predictable over time?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `latency_extended_1x` | **Primary** | `p99_ms` (time series) | 5-minute run to detect Jitter, GC pauses, or thermal throttling. |
| `latency_short_1x` | Secondary | `cv` (coef. of variation) | Quick check for immediate instability. |

#### 2.4.4. Resource Efficiency

**The Question:** What is the infrastructure cost per unit of work?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `concurrency_short_1x` | **Primary** | `memory_peak` | **Purpose:** Isolates connection management costs and per-connection memory overhead (5k connections). |
| `throughput_short_1x` | Secondary | `cpu_usage` / `rps` | Calculates CPU efficiency at saturation. |

#### 2.4.5. Stress Behavior (Under Load)

**The Question:** How does the system degrade when pushed beyond its limit?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `churn_short_1x` | **Primary** | `errors`, `achieved_rps` | Stresses accept queue and handshake logic (Connection Storm). |
| `concurrency_short_1x` | Secondary | `errors` | Checks for file descriptor exhaustion or OOM kills under connection pressure. |

#### 2.4.6. Cross-Scenario Consistency

**The Question:** Does the system perform reliably across different traffic patterns?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `throughput_short_1x` | Secondary | `achieved_rps` | Compared against `latency_short_1x` to quantify "Usable Capacity" vs "Max Capacity". |

#### 2.4.7. Payload Matrix (Workstation)

Workstation runs a payload matrix for `throughput_short_1x`, `latency_short_1x`, and `latency_extended_1x` at `64B` and `4KiB`.

### 2.5. Load Generation & Tooling

The choice of load generator is dictated strictly by the dimension being measured.

#### Closed-Loop (`wrk`)
**Used for:** *Performance Ceiling*, *Stress Behavior*
- **Why:** Naturally finds the system's maximum equilibrium throughput.
- **Limitation:** Subject to Coordinated Omission; not for latency measurement.

#### Open-Loop (`bench-loadgen`)
**Used for:** *Tail Latency Quality*, *Stability*
- **Why:** Controls arrival rate independent of system speed, exposing true queuing delays.

### 2.6. Test Environment & Isolation

To satisfy the **Stability & Variance** and **Resource Efficiency** dimensions, the environment must strictly control non-proxy variables.

#### Mandatory Isolation Constraints
1.  **Loadgen Isolation (1 core):** Load generator must not compete with upstream or proxy.
2.  **Backend Isolation (1 core):** The upstream service must never compete for CPU cycles with the proxy.
3.  **Proxy Pinning (2 cores):** Required for accurate CPU/RPS ratios and to prevent OS scheduler noise.
4.  **Proxy Memory Limit:** Workstation runs must cap proxy memory at 1GiB.
5.  **Deterministic Upstream:** The backend must respond in constant time to ensure variance is attributable solely to the proxy.

### 2.7. Metrics Interpretation Rules

Metrics must be interpreted within the context of their specific dimension.

-   **Throughput (RPS):** Valid for Performance Ceiling and Stress Behavior.
-   **Latency (P99):** Valid for Tail Latency Quality. Authoritative for Standalone Mode.
-   **Errors:** Valid for Stress Behavior. Fatal (invalidates run) for Baseline/Latency tests.

### 2.8. Non-Goals & Explicit Exclusions

*   **Control Plane Overhead:** Standalone mode explicitly excludes the cost of configuration distribution or agent-driven updates.
*   **Protocol Breadth:** Currently HTTP/1.1 only.
*   **Lifecycle Behavior:** Operational events like reloads or failovers are not measured in this mode.

---

## 3. System / Kubernetes Cases

### 3.1. Introduction

This section defines the evaluation framework for **System / Kubernetes Mode**. 

System Mode benchmarks the Pavis ecosystem as a holistic unit, including both the data plane (Runtime) and the control plane (Relay). It evaluates how the system behaves under operational lifecycle events, such as configuration updates, rollouts, and recovery scenarios.

System Mode is **lifecycle-oriented**. It does not focus on raw throughput or micro-performance ranking, but on the reliability and predictability of the system during management actions.

### 3.2. Execution Environment

System benchmarks are executed in a controlled Kubernetes environment using **kind** (Kubernetes in Docker). The environment includes the following mandatory components:

*   **Pavis Runtime:** The data-plane proxy under test, running as a sidecar or gateway.
*   **Relay:** The control-plane component responsible for artifact distribution.
*   **Deterministic Upstream:** A backend service providing predictable response characteristics.
*   **Load Generator:** An external tool providing steady-state traffic.

Control-plane participation is an intentional and required part of this evaluation mode.

CI executions (when run) are **non-authoritative** and must not be used for cross-proxy comparisons or public performance claims.

### 3.3. Mode Boundary Rules

*   **Lifecycle Focus:** System Mode explicitly measures the impact of control-plane operations on the data plane.
*   **No Throughput Ranking:** System Mode results are not intended for ranking proxy performance. Throughput is a secondary metric used only to establish a baseline for measuring event impact.
*   **Isolation from Micro-Performance:** Results from System Mode represent system-level behavior and are not comparable to isolated dataplane benchmarks.

### 3.4. System-Level Benchmark Cases

#### 3.4.1. Configuration Reload Convergence
*   **Dimension:** Operational Characteristics (#6)
*   **Description:** The Relay pushes a new precompiled configuration artifact to a running Runtime instance. The Runtime performs an atomic data-plane switch to the new configuration while processing active traffic.
*   **Metrics:**
    *   **Convergence Time:** Duration from Relay publication to the first request processed by the new configuration.
    *   **P99 Latency Delta:** The variance in tail latency observed during the reload window compared to the steady-state baseline.
    *   **Error / Drop Count:** Any requests failed or dropped during the transition.

#### 3.4.2. Rollback & Recovery
*   **Dimensions:** Operational Characteristics (#6), Recovery & Hysteresis (Derived B)
*   **Description:** A configuration update is pushed and, after a short period of observation, is reverted to the previous known-good state. This simulates a failed deployment recovery.
*   **Metrics:**
    *   **Time to Restore Baseline:** Duration required for P99 latency to return to the pre-update baseline after rollback.
    *   **Memory Stabilization:** Observation of RSS behavior to ensure no resources are leaked during the double-switch.
    *   **Error Amplification:** Detection of any compounded errors during the rapid configuration changes.

#### 3.4.3. Stress → Recovery
*   **Dimensions:** Stress Behavior (#5, system view), Recovery & Hysteresis (Derived B)
*   **Description:** The system is subjected to load exceeding its capacity. After a defined period, the load is removed or returned to a sustainable level.
*   **Metrics:**
    *   **P99 Recovery Time:** The time taken for tail latency to normalize after the stressor is removed.
    *   **RSS Return-to-Baseline:** Monitoring if memory usage returns to steady-state levels or exhibits hysteresis.

#### 3.4.4. Durability / Soak Test
*   **Dimension:** Release Gate
*   **Description:** The system is operated under a continuous, steady-state load for a multi-hour duration.
*   **Metrics:**
    *   **Memory Monotonicity:** Analysis of RSS trends to detect slow-growth leaks.
    *   **Latency Drift:** Comparison of P99 at the start versus the end of the window.
    *   **Resource Leakage Indicators:** Monitoring of system handles (file descriptors, threads) for linear growth.

### 3.5. Metrics Rules

*   **Event Correlation:** All metrics in System Mode MUST be reported as event-correlated time series.
*   **Relative Percentiles:** Latency percentiles are valid only when interpreted relative to a steady-state baseline established within the same environment.
*   **Fatal Errors:** Any unforced errors observed during steady-state baseline periods invalidate the benchmark run.

### 3.6. Explicit Non-Goals

System Mode does **NOT** aim to:
*   Rank proxies by performance against competitors.
*   Measure or report raw throughput maximums.
*   Evaluate micro-architectural dataplane efficiency.

---

## 4. Fairness & Configuration Equivalence

### 4.1. Purpose

This section ensures all proxies in the benchmark (Pavis, Envoy, Nginx, HAProxy) are configured with equivalent semantics to enable fair performance comparison.

We strictly adhere to a **"Fairness Standard"** where proxies are unthrottled and given equal access to available resources (CPU/RAM/Connections) within the container limits.

---

### 4.2. Configuration Equivalence Table

The following table maps the semantic behaviors across all tested proxies.

| Semantic Behavior                  | Pavis                                      | Envoy                                      | Nginx                                      | HAProxy                                    |
|------------------------------------|--------------------------------------------|--------------------------------------------|--------------------------------------------|--------------------------------------------|
| **Workers/Threads**                | Runtime-detected (2 expected)              | `--concurrency 2`                          | `worker_processes 2`                       | `nbthread 2`                               |
| **Worker CPU Affinity**            | Runtime (OS scheduler)                     | Runtime (OS scheduler)                     | Runtime (OS scheduler)                     | `cpu-map 1 0`, `cpu-map 2 1`               |
| **Downstream Keepalive Enabled**   | ✅ Enabled (default)                       | ✅ Enabled (default)                       | ✅ `keepalive_timeout 65`                  | ✅ Enabled (HTTP mode default)             |
| **Downstream Keepalive Timeout**   | 30s (assumed default)                      | 3600s (route timeout, can be overridden)   | `keepalive_timeout 65`                     | `timeout client 30s`                       |
| **Downstream Keepalive Requests**  | Unlimited (assumed)                        | Unlimited (default)                        | `keepalive_requests 10000`                 | Unlimited (default)                        |
| **Upstream Keepalive Enabled**     | ✅ Connection pool (default)               | ✅ Connection pool (cluster)               | ✅ `keepalive 1000` (upstream)             | ✅ Enabled (default)                       |
| **Upstream Connection Pool Size**  | Runtime-managed                            | Cluster config (circuit breaker)           | `keepalive 1000` (persistent pool)         | No explicit limit                          |
| **HTTP Version (Downstream)**      | HTTP/1.1                                   | HTTP/1.1                                   | HTTP/1.1 (default)                         | HTTP/1.1 (HTTP mode)                       |
| **HTTP Version (Upstream)**        | HTTP/1.1                                   | HTTP/1.1                                   | `proxy_http_version 1.1`                   | HTTP/1.1 (HTTP mode)                       |
| **Connection Header (Upstream)**   | `Connection: keep-alive` (implicit)        | Managed by cluster                         | `proxy_set_header Connection ""`           | Managed by backend config                  |
| **Max Concurrent Connections**     | No explicit limit (OS-limited)             | No explicit limit                          | `worker_connections 65535` (per worker)    | `maxconn 20000` (global)                   |
| **Idle Timeout (Upstream)**        | 30s (assumed)                              | Connection pool idle timeout               | Persistent (with keepalive)                | `timeout server 30s`                       |
| **Connect Timeout (Upstream)**     | Default (5s assumed)                       | Default                                    | Default                                    | `timeout connect 5s`                       |
| **Logging**                        | ⛔ Disabled for benchmark                  | ⛔ `/dev/null`                             | ⛔ `access_log off; error_log /dev/null`   | ⛔ `no log`                                |
| **TCP Optimizations**              | OS defaults                                | OS defaults                                | `tcp_nopush on; tcp_nodelay on`            | OS defaults                                |
| **Event Model**                    | Async (Rust tokio)                         | Event-driven (C++ libevent)                | `use epoll; multi_accept on`               | Event-driven (C epoll)                     |
| **Worker Connections Limit**       | OS ulimit (`ulimit -n`)                    | OS ulimit                                  | `worker_connections 65535`                 | `maxconn 20000`                            |

---

### 4.3. Detailed Configuration Analysis

#### Worker/Thread Count
**Equivalence**: All proxies are configured with **2 workers/threads** to match the baseline resource profile (2 CPUs).
- **Pavis**: Automatically detects available CPUs (container limit).
- **Envoy**: `--concurrency 2` flag.
- **Nginx**: `worker_processes 2`.
- **HAProxy**: `nbthread 2`.

#### Keepalive Configuration
**Downstream (Client → Proxy)**:
- All proxies support persistent connections.
- Timeouts vary slightly (30s - 3600s) but are sufficient for the 30s benchmark duration.

**Upstream (Proxy → Backend)**:
- **Pavis**: Runtime-managed connection pool.
- **Envoy**: Cluster circuit breaker config.
- **Nginx**: `upstream { keepalive 1000; }` (Increased from 100 to prevent bottlenecks).
- **HAProxy**: Unlimited server connections.

#### Logging Overhead
**Requirement**: All proxies must disable access logging to eliminate Disk I/O overhead.
- **Pavis**: Logging disabled by default in benchmark mode.
- **Envoy**: `/dev/null`.
- **Nginx**: `access_log off`.
- **HAProxy**: `no log`.

#### Nginx-Specific Optimizations
To ensure Nginx is not unfairly penalized:
- **Connections**: `worker_connections` increased to `65535`.
- **TCP**: `tcp_nopush on` and `tcp_nodelay on` are enabled (standard best practice).
- **Event Model**: `use epoll` and `multi_accept on` are enabled.

---

### 4.4. Validation Checklist

Before running benchmarks, verify the following:

- [ ] All proxies use 2 workers/threads.
- [ ] Downstream & Upstream keepalive is enabled.
- [ ] HTTP/1.1 is used for all connections.
- [ ] Logging is disabled.
- [ ] CPU pinning is active (`cpuset_cpus` in docker-compose).
- [ ] **Host `ulimit -n` is ≥ 65535** (Crucial for high concurrency tests).
- [ ] CPU governor is set to `performance`.

---

### 4.5. Reporting Fairness Violations

If you identify a configuration mismatch that affects fairness (e.g., one proxy has an unfair advantage or handicap):

1. **Document the discrepancy**: Which setting is different?
2. **Assess impact**: Does it materially affect RPS or Latency?
3. **Open an Issue**: https://github.com/fabian4/pavis/issues

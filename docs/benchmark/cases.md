# Pavis Benchmark Cases

## 1. Introduction

This document operationalizes the evaluation framework defined in [BENCHMARK_METHODOLOGY.md](./METHODOLOGY.md). While the methodology defines *what* dimensions determine production readiness, this document specifies *how* we measure them using concrete, reproducible test cases.

Every benchmark case listed below exists solely to satisfy one or more of the 7 Core Evaluation Dimensions. We do not run "generic" load tests; every CPU cycle spent benchmarking must answer a specific engineering question regarding capacity, latency, stability, or efficiency.

---

## 2. Dimension-Indexed Benchmark Coverage

### 2.1. Performance Ceiling (Capacity)

**The Question:** At what point does the system fail to process requests successfully?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `throughput_short_1x` | **Primary** | `achieved_rps` | Measures absolute max packet forwarding rate (saturation). |
| `concurrency_short_1x` | Secondary | `achieved_rps` | Verifies capacity degradation under high connection count. |

### 2.2. Tail Latency Quality

**The Question:** What is the worst-case experience for a user request?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `latency_short_1x` | **Primary** | `p99_ms`, `p99.9_ms` | Standard baseline at sustainable load (default 10k RPS). |
| `latency_extended_1x` | Secondary | `max_ms` | captures rare outliers over longer observation windows. |

**Note on Latency Metrics:** `p99_ms` is the authoritative SLA metric for all tail latency evaluations. `p99.9_ms` is collected for exploratory and diagnostic purposes and may not appear in all summary reports.

### 2.3. Stability & Variance

**The Question:** Is performance predictable over time?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `latency_extended_1x` | **Primary** | `p99_ms` (time series) | 5-minute run to detect Jitter, GC pauses, or thermal throttling. |
| `latency_short_1x` | Secondary | `cv` (coef. of variation) | Quick check for immediate instability. |

**Note on Stability Metrics:** While `cv` (coefficient of variation) is used as a coarse instability signal during initial analysis, percentile-based dispersion (e.g., p99 Interquartile Range) is authoritative for real-world latency variance analysis.

### 2.4. Resource Efficiency

**The Question:** What is the infrastructure cost per unit of work?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `concurrency_short_1x` | **Primary** | `memory_peak` | Isolates per-connection memory overhead (5k idle connections). |
| `throughput_short_1x` | Secondary | `cpu_usage` / `rps` | Calculates CPU efficiency at saturation. |

### 2.5. Stress Behavior (Under Load)

**The Question:** How does the system degrade when pushed beyond its limit?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `churn_short_1x` | **Primary** | `errors`, `achieved_rps` | Stresses accept queue and handshake logic (Connection Storm). |
| `concurrency_short_1x` | Secondary | `errors` | Checks for file descriptor exhaustion or OOM kills. |

### 2.6. Operational Characteristics

**The Question:** What is the impact of control-plane operations on data-plane traffic?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `reload_short_1x` | **Primary** | `p99_ms` (delta) | Measures latency spike during hot configuration reload. |

### 2.7. Cross-Scenario Consistency

**The Question:** Does the system perform reliably across different traffic patterns?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `throughput_short_1x` | Secondary | `achieved_rps` | Compared against `latency_short_1x` to quantify "Usable Capacity" vs "Max Capacity". |

---

## 3. Load Generation & Tooling

The choice of load generator is dictated strictly by the dimension being measured.

### Closed-Loop (`wrk`)
**Used for:** *Performance Ceiling*, *Stress Behavior*
- **Why:** We need to saturate the proxy. In closed-loop systems, the client waits for a response before sending the next request. This naturally finds the system's maximum equilibrium throughput without needing manual rate tuning.
- **Limitation:** Cannot measure true Latency (suffer from Coordinated Omission).

### Open-Loop (`bench-loadgen` / `wrk2`)
**Used for:** *Tail Latency Quality*, *Stability*, *Operational Characteristics*
- **Why:** To measure latency, we must control the arrival rate independent of the system's processing speed. Open-loop generators send requests at a fixed schedule (inter-arrival time), exposing queuing delays that closed-loop tools hide.

---

## 4. Test Environment & Isolation

To satisfy the **Stability & Variance** and **Resource Efficiency** dimensions, the environment must strictly control non-proxy variables.

### Mandatory Isolation Constraints
1.  **Backend Isolation (CPU 0):** The upstream service must never compete for CPU cycles with the proxy. Contention here invalidates *Tail Latency* results.
2.  **Proxy Pinning (CPU 1-2):** The proxy must be pinned to specific cores. This is required for *Resource Efficiency* to calculate accurate CPU/RPS ratios and to prevent OS scheduler noise from polluting *Stability* metrics.
3.  **Deterministic Upstream:** The backend must respond in constant time (or controlled distribution) to ensure that measured variance is attributable solely to the proxy.

---

## 5. Metrics Interpretation Rules

Metrics must be interpreted within the context of their specific dimension.

-   **Throughput (RPS):**
    -   *Valid* for: Performance Ceiling, Stress Behavior.
    -   *Ignored* for: Latency Quality (we fix the RPS, so "achieved RPS" is just a sanity check).
-   **Latency (P99):**
    -   *Valid* for: Tail Latency, Operational Characteristics.
    -   *Invalid* for: Performance Ceiling (at saturation, latency is dominated by queuing and is mathematically unbounded).
-   **Errors:**
    -   *Valid* for: Stress Behavior.
    -   *Fatal (invalidates the benchmark run)* for: All other dimensions (any error invalidates a baseline capacity or latency test).

---

## 6. Non-Goals & Explicit Exclusions

This benchmark suite focuses on the core data-plane properties defined in the methodology. It explicitly excludes:

-   **Protocol Breadth:** HTTP/2 and gRPC are not currently covered (HTTP/1.1 only).
-   **Soak Testing:** Multi-hour runs for *Durability* (Release Gate) are handled by separate CI pipelines, not this suite.
-   **Feature Tax:** TLS (mTLS) and Observability overheads are measured via specific configuration variants of these cases, not by unique case definitions.
# Pavis Benchmark Methodology

## 1. Introduction

Naive benchmarking of network proxies focuses almost exclusively on "max throughput" (Requests Per Second) under ideal conditions. While useful for marketing, this metric is insufficient for engineering robust service mesh infrastructure. A proxy that pushes 100k RPS but imposes 500ms tail latency during garbage collection, or consumes 4GB of RAM to handle a traffic spike, is operationally unfit for production.

This document establishes a rigorous, multidimensional framework for evaluating **production-grade service mesh sidecars**. It distinguishes between *kernel development benchmarks* (micro-benchmarks of packet forwarding) and *productization benchmarks* (macro-benchmarks of full protocol stacks).

Our goal is not merely to measure speed, but to quantify predictability, efficiency, and safety. All future performance evaluations must adhere to the dimensions and constraints defined herein.

---

## 2. Benchmark Execution Modes

To ensure scientific rigor and clarity of purpose, all Pavis benchmarks are executed in one of two distinct modes.

### 2.1. Standalone Dataplane Mode
*   **Purpose:** Measure the **intrinsic performance** of the data plane in isolation.
*   **Environment:** Minimal Docker or bare-metal environment.
*   **Constraints:** Static configuration only; no control-plane components (Relay, Config Agent) are present.
*   **Target Dimensions:** Capacity (#1), Tail Latency (#2), Stability (#3), Resource Efficiency (#4), Stress Behavior (#5), and Consistency (#7).
*   **Comparability:** Primary mode for benchmarking Pavis against industry-standard proxies (Envoy, Nginx).

### 2.2. System / Kubernetes Mode
*   **Purpose:** Measure **control-plane assisted lifecycle behavior** and system-wide reliability.
*   **Environment:** Kubernetes (kind) cluster.
*   **Constraints:** Includes full system components (Relay, Agent); configuration is dynamic and pushed during tests.
*   **Target Dimensions:** Operational Characteristics (#6), Recovery (#B), and Durability (#Gate).
*   **Comparability:** Architecture-specific; measures the maturity of the Pavis ecosystem rather than micro-performance.

### 2.3. Execution Profiles & Authority
Benchmark execution is further constrained by environment profile.

*   **github (CI-only):** Pavis-only regression signal; skips `latency_extended_1x`. Reports are generated from `summary.csv` and are **non-authoritative** due to shared runner variance.
*   **workstation (authoritative):** Dedicated hardware required. CPU pinning is mandatory with a 4-core allocation (1 loadgen/wrk, 1 upstream, 2 proxy) and a 1GiB proxy memory limit. Standalone payload matrix runs `throughput_short_1x`, `latency_short_1x`, and `latency_extended_1x` at `64B` and `4KiB`.

---

## 3. Core Evaluation Dimensions

The following seven dimensions constitute the primary axes of evaluation. Every comprehensive benchmark suite must address these dimensions to provide a complete performance profile. The dimensions are invariant across execution modes.

### 2.1. Performance Ceiling (Capacity)
*   **The Question:** At what point does the system fail to process requests successfully?
*   **Production Relevance:** Determines the absolute maximum capacity of a standard unit of infrastructure (e.g., 1 CPU core), guiding capacity planning and autoscaling triggers.
*   **Primary Metrics:**
    *   **Max Sustainable RPS:** The highest load where success rate is >99.9% and P99 latency remains within defined SLOs.
    *   **Saturation Point:** The load at which CPU reaches 100% utilization.
*   **Typical Cases:** Throughput saturation tests with minimal payload logic.

### 2.2. Tail Latency Quality
*   **The Question:** What is the worst-case experience for a user request?
*   **Production Relevance:** In microservices architectures, tail latencies compound across call chains. A "fast average" with a "slow tail" results in unacceptable system-wide performance.
*   **Primary Metrics:**
    *   **P99 and P99.9 Latency:** Measured in milliseconds.
    *   **Max Latency:** The absolute worst-case request time during the sample window.
*   **Typical Cases:** Fixed-rate load testing (below saturation) measuring distribution curves. Note: Averages (mean) are explicitly discarded as a primary metric.

### 2.3. Stability & Variance
*   **The Question:** Is performance predictable over time?
*   **Production Relevance:** Jitter makes autoscaling algorithms unstable and causes "thundering herd" issues in upstream services. A boring proxy is a good proxy.
*   **Primary Metrics:**
    *   **Latency Standard Deviation:** Deviation from the mean.
    *   **Coefficient of Variation (CV):** Ratio of standard deviation to the mean.
*   **Clarification:** Latency distributions in networked systems are non-normal. While mean-based statistics are provided for legacy baseline comparison, percentile-based dispersion (e.g., P99 Interquartile Range) is more representative of real-world variance and is preferred in implementation analysis.
*   **Typical Cases:** Steady-state load testing over medium duration (e.g., 10 minutes) analyzing time-series consistency.

### 2.4. Resource Efficiency
*   **The Question:** What is the infrastructure cost per unit of work?
*   **Production Relevance:** Directly dictates the Total Cost of Ownership (TCO) of the service mesh.
*   **Primary Metrics:**
    *   **CPU/RPS Ratio:** Millicores consumed per 1,000 requests.
    *   **Memory Footprint:** RSS memory usage at specific load tiers.
*   **Typical Cases:** Ramping load tests correlating system telemetry with application throughput.

### 2.5. Stress Behavior (Under Load)
*   **The Question:** How does the system degrade when pushed beyond its limit?
*   **Production Relevance:** Systems must degrade gracefully. Hard crashes, hanging connections, or memory exhaustion (OOM) during traffic spikes cause cascading failures.
*   **Primary Metrics:**
    *   **Goodput vs. Offered Load:** Does the system shed load or queue it indefinitely?
    *   **Error Distribution:** Immediate fast-fail (503) vs. connection timeouts.
*   **Typical Cases:** Overload tests (150% of capacity) and "spike" tests.

### 2.6. Operational Characteristics
*   **The Question:** What is the impact of control-plane operations on data-plane traffic?
*   **Production Relevance:** Dynamic environments constantly push configuration updates. These updates must not cause packet drops or latency spikes.
*   **Primary Metrics:**
    *   **Reload Latency Impact:** Delta in P99 latency during configuration reload.
    *   **Convergence Time:** Time from config push to traffic shifting.
*   **Typical Cases:** Hot-restart tests, certificate rotation simulation, and dynamic upstream reconfiguration during load.

### 2.7. Cross-Scenario Consistency
*   **The Question:** Does the system perform reliably across different traffic patterns?
*   **Production Relevance:** Ensures the proxy is general-purpose and not over-optimized for a single synthetic use case (e.g., 64-byte payloads).
*   **Primary Metrics:**
    *   **Performance Delta:** Variance in throughput/latency between small (1KB) and large (1MB) payloads.
    *   **Protocol Overhead:** degradation moving from HTTP/1.1 to HTTP/2 or gRPC.
*   **Typical Cases:** Matrix testing across payload sizes, connection keep-alive settings, and protocol versions.

---

## 4. Derived Dimensions

Derived dimensions are not independent tests; they are specific analytical cuts of data gathered during the Core dimension tests.

### Derived Dimension A: Feature Tax
**Feature Tax** is the quantified performance cost of enabling production-essential features compared to a "naked" baseline.

*   **Definition:** `(Performance_Baseline - Performance_Feature) / Performance_Baseline`
*   **Mandatory Analysis Areas:**
    *   **TLS Tax:** The cryptographic overhead of HTTPS versus plain HTTP. This comparison may include standard HTTPS or be extended to mutual TLS (mTLS) to evaluate the additional handshake and certificate validation costs.
    *   **Observability Tax:** The CPU/Latency cost of generating high-cardinality metrics and distributed tracing spans.
*   **Usage:** Feature Tax should be applied to *Performance Ceiling* and *Resource Efficiency* analysis to set realistic production expectations.

### Derived Dimension B: Recovery & Hysteresis
**Recovery** measures the system's ability to return to baseline state *after* a stress event has concluded.

*   **Definition:** Time-series analysis of the cool-down period following a Stress Behavior test.
*   **Mandatory Analysis Areas:**
    *   **Memory Hysteresis:** Does RSS return to baseline, or does fragmentation keep memory usage high?
    *   **Latency Normalization:** How many seconds until P99 returns to steady-state levels after the load spike ends?
*   **Relevance:** Critical for long-running processes where temporary spikes must not permanently degrade the runtime environment.

---

## 5. Release Gate Dimension

This dimension is computationally expensive and is reserved for major release candidates or publication checkpoints, rather than daily CI runs.

### Durability / Soak Testing
*   **The Question:** Does the system degrade over extended periods of operation?
*   **Production Relevance:** Detects slow memory leaks, resource handle exhaustion (file descriptors), and integer overflow issues that micro-benchmarks miss.
*   **Scope:** Multi-hour (e.g., 8+ hours) continuous load runs.
*   **Pass Criteria:**
    *   **Zero Monotonic Growth:** Memory usage must plateau and not show linear growth trend.
    *   **Stable Latency:** P99 at the end of the window must be within statistically significant range of P99 at the start.
    *   **Resource Reclamation:** File descriptors must verify full closure cycles.

---

## 6. Methodology Guardrails

To ensure data integrity and comparability, the following practices are strictly enforced:

1.  **No Apple-to-Orange Aggregation:** Never average metrics across dissimilar test cases (e.g., do not average "1KB Payload" results with "100KB Payload" results into a single score).
2.  **Separate Open vs. Closed Loop:** Clearly distinguish between closed-loop (waiting for response) and open-loop (fire-and-forget) load generation. Closed-loop testing hides latency degradation (Coordinated Omission); Open-loop is required for accurate tail latency measurement.
3.  **Data Provenance:** Every graph must be backed by accessible tabular data. Visualizations without raw numbers are rejected.
4.  **Isolation:** Benchmarks must run on dedicated, isolated hardware/nodes. No noisy neighbors.
5.  **Reproducibility:** All configuration (OS tuning, proxy config, load generator command) must be committed as code alongside results.

---

## 7. Intended Outcomes

By adhering to this framework, we ensure:
*   **Accountability:** Architecture decisions are backed by data, not intuition.
*   **Standardization:** A shared language for discussing performance across engineering teams.
*   **Integrity:** Prevention of "benchmark gaming" or cherry-picking favorable scenarios.

This methodology defines the standard of quality required for the project to be considered production-ready.

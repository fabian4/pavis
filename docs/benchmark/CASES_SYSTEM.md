# Pavis Benchmark Cases (System / Kubernetes)

## 1. Introduction

This document defines the evaluation framework for **System / Kubernetes Mode**. 

System Mode benchmarks the Pavis ecosystem as a holistic unit, including both the data plane (Runtime) and the control plane (Relay). It evaluates how the system behaves under operational lifecycle events, such as configuration updates, rollouts, and recovery scenarios.

System Mode is **lifecycle-oriented**. It does not focus on raw throughput or micro-performance ranking, but on the reliability and predictability of the system during management actions.

## 2. Execution Environment

System benchmarks are executed in a controlled Kubernetes environment using **kind** (Kubernetes in Docker). The environment includes the following mandatory components:

*   **Pavis Runtime:** The data-plane proxy under test, running as a sidecar or gateway.
*   **Relay:** The control-plane component responsible for artifact distribution.
*   **Deterministic Upstream:** A backend service providing predictable response characteristics.
*   **Load Generator:** An external tool providing steady-state traffic.

Control-plane participation is an intentional and required part of this evaluation mode.

CI executions (when run) are **non-authoritative** and must not be used for cross-proxy comparisons or public performance claims.

## 3. Mode Boundary Rules

*   **Lifecycle Focus:** System Mode explicitly measures the impact of control-plane operations on the data plane.
*   **No Throughput Ranking:** System Mode results are not intended for ranking proxy performance. Throughput is a secondary metric used only to establish a baseline for measuring event impact.
*   **Isolation from Micro-Performance:** Results from System Mode represent system-level behavior and are not comparable to isolated dataplane benchmarks.

## 4. System-Level Benchmark Cases

### 4.1. Configuration Reload Convergence
*   **Dimension:** Operational Characteristics (#6)
*   **Description:** The Relay pushes a new precompiled configuration artifact to a running Runtime instance. The Runtime performs an atomic data-plane switch to the new configuration while processing active traffic.
*   **Metrics:**
    *   **Convergence Time:** Duration from Relay publication to the first request processed by the new configuration.
    *   **P99 Latency Delta:** The variance in tail latency observed during the reload window compared to the steady-state baseline.
    *   **Error / Drop Count:** Any requests failed or dropped during the transition.

### 4.2. Rollback & Recovery
*   **Dimensions:** Operational Characteristics (#6), Recovery & Hysteresis (Derived B)
*   **Description:** A configuration update is pushed and, after a short period of observation, is reverted to the previous known-good state. This simulates a failed deployment recovery.
*   **Metrics:**
    *   **Time to Restore Baseline:** Duration required for P99 latency to return to the pre-update baseline after rollback.
    *   **Memory Stabilization:** Observation of RSS behavior to ensure no resources are leaked during the double-switch.
    *   **Error Amplification:** Detection of any compounded errors during the rapid configuration changes.

### 4.3. Stress → Recovery
*   **Dimensions:** Stress Behavior (#5, system view), Recovery & Hysteresis (Derived B)
*   **Description:** The system is subjected to load exceeding its capacity. After a defined period, the load is removed or returned to a sustainable level.
*   **Metrics:**
    *   **P99 Recovery Time:** The time taken for tail latency to normalize after the stressor is removed.
    *   **RSS Return-to-Baseline:** Monitoring if memory usage returns to steady-state levels or exhibits hysteresis.

### 4.4. Durability / Soak Test
*   **Dimension:** Release Gate
*   **Description:** The system is operated under a continuous, steady-state load for a multi-hour duration.
*   **Metrics:**
    *   **Memory Monotonicity:** Analysis of RSS trends to detect slow-growth leaks.
    *   **Latency Drift:** Comparison of P99 at the start versus the end of the window.
    *   **Resource Leakage Indicators:** Monitoring of system handles (file descriptors, threads) for linear growth.

## 5. Metrics Rules

*   **Event Correlation:** All metrics in System Mode MUST be reported as event-correlated time series.
*   **Relative Percentiles:** Latency percentiles are valid only when interpreted relative to a steady-state baseline established within the same environment.
*   **Fatal Errors:** Any unforced errors observed during steady-state baseline periods invalidate the benchmark run.

## 6. Explicit Non-Goals

System Mode does **NOT** aim to:
*   Rank proxies by performance against competitors.
*   Measure or report raw throughput maximums.
*   Evaluate micro-architectural dataplane efficiency.

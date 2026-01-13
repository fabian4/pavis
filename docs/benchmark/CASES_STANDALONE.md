# Pavis Benchmark Cases (Standalone Dataplane)

## 1. Introduction

This document operationalizes the evaluation framework defined in [METHODOLOGY.md](./METHODOLOGY.md) for **Standalone Dataplane Mode**. 

Standalone mode is designed to measure the **intrinsic performance properties** of the Pavis data plane in isolation. It focuses on the fundamental cost of packet processing, routing, and transport management without interference or assistance from control-plane components.

## 2. Mode Boundary Rules

To ensure measurement integrity, the following rules are strictly enforced for all standalone benchmarks:

*   **Static Configuration:** The proxy under test MUST be initialized with a fixed configuration. No configuration updates are permitted during the measurement window.
*   **Isolation:** The environment must be isolated to prevent interference from non-dataplane processes.
*   **No External Dependencies:** Standalone benchmarks must not depend on external configuration distribution or discovery services during execution.

## 3. Execution Profiles

Standalone mode supports two profiles:

*   **github:** CI-only regression signal for Pavis; skips `latency_extended_1x` and produces reports derived from `summary.csv` only.
*   **workstation:** Authoritative runs on dedicated hardware with strict CPU pinning and resource limits.

## 4. Dimension-Indexed Benchmark Coverage

### 4.1. Performance Ceiling (Capacity)

**The Question:** At what point does the system fail to process requests successfully?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `throughput_short_1x` | **Primary** | `achieved_rps` | Measures absolute max packet forwarding rate (saturation) under static configuration. |
| `concurrency_short_1x` | Secondary | `achieved_rps` | Verifies throughput degradation when forced to manage high connection counts. |

### 4.2. Tail Latency Quality

**The Question:** What is the worst-case experience for a user request?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `latency_short_1x` | **Primary** | `p99_ms`, `p99.9_ms` | Standard baseline at sustainable load (default 10k RPS). |
| `latency_extended_1x` | Secondary | `p99_ms` | **Authoritative:** uses percentile-based metrics to identify tail latency trends; `max_ms` is diagnostic only. |

### 4.3. Stability & Variance

**The Question:** Is performance predictable over time?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `latency_extended_1x` | **Primary** | `p99_ms` (time series) | 5-minute run to detect Jitter, GC pauses, or thermal throttling. |
| `latency_short_1x` | Secondary | `cv` (coef. of variation) | Quick check for immediate instability. |

### 4.4. Resource Efficiency

**The Question:** What is the infrastructure cost per unit of work?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `concurrency_short_1x` | **Primary** | `memory_peak` | **Purpose:** Isolates connection management costs and per-connection memory overhead (5k connections). |
| `throughput_short_1x` | Secondary | `cpu_usage` / `rps` | Calculates CPU efficiency at saturation. |

### 4.5. Stress Behavior (Under Load)

**The Question:** How does the system degrade when pushed beyond its limit?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `churn_short_1x` | **Primary** | `errors`, `achieved_rps` | Stresses accept queue and handshake logic (Connection Storm). |
| `concurrency_short_1x` | Secondary | `errors` | Checks for file descriptor exhaustion or OOM kills under connection pressure. |

### 4.6. Cross-Scenario Consistency

**The Question:** Does the system perform reliably across different traffic patterns?

| Benchmark Case | Role | Metrics Used | Notes |
| :--- | :--- | :--- | :--- |
| `throughput_short_1x` | Secondary | `achieved_rps` | Compared against `latency_short_1x` to quantify "Usable Capacity" vs "Max Capacity". |

### 4.7. Payload Matrix (Workstation)

Workstation runs a payload matrix for `throughput_short_1x`, `latency_short_1x`, and `latency_extended_1x` at `64B` and `4KiB`.

## 5. Load Generation & Tooling

The choice of load generator is dictated strictly by the dimension being measured.

### Closed-Loop (`wrk`)
**Used for:** *Performance Ceiling*, *Stress Behavior*
- **Why:** Naturally finds the system's maximum equilibrium throughput.
- **Limitation:** Subject to Coordinated Omission; not for latency measurement.

### Open-Loop (`bench-loadgen`)
**Used for:** *Tail Latency Quality*, *Stability*
- **Why:** Controls arrival rate independent of system speed, exposing true queuing delays.

## 6. Test Environment & Isolation

To satisfy the **Stability & Variance** and **Resource Efficiency** dimensions, the environment must strictly control non-proxy variables.

### Mandatory Isolation Constraints
1.  **Loadgen Isolation (1 core):** Load generator must not compete with upstream or proxy.
2.  **Backend Isolation (1 core):** The upstream service must never compete for CPU cycles with the proxy.
3.  **Proxy Pinning (2 cores):** Required for accurate CPU/RPS ratios and to prevent OS scheduler noise.
4.  **Proxy Memory Limit:** Workstation runs must cap proxy memory at 1GiB.
5.  **Deterministic Upstream:** The backend must respond in constant time to ensure variance is attributable solely to the proxy.

## 7. Metrics Interpretation Rules

Metrics must be interpreted within the context of their specific dimension.

-   **Throughput (RPS):** Valid for Performance Ceiling and Stress Behavior.
-   **Latency (P99):** Valid for Tail Latency Quality. Authoritative for Standalone Mode.
-   **Errors:** Valid for Stress Behavior. Fatal (invalidates run) for Baseline/Latency tests.

## 8. Non-Goals & Explicit Exclusions

*   **Control Plane Overhead:** Standalone mode explicitly excludes the cost of configuration distribution or agent-driven updates.
*   **Protocol Breadth:** Currently HTTP/1.1 only.
*   **Lifecycle Behavior:** Operational events like reloads or failovers are not measured in this mode.

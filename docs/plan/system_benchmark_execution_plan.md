# Execution Plan: System / Kubernetes (kind) Benchmark Suite

This document defines the implementation strategy for **System / Kubernetes Mode** benchmarks, focusing on sidecar lifecycle behavior, configuration convergence, and system-level durability.

## 1. Scope & Core Constraints

### 1.1. Mandatory Mode Declaration
All benchmarks described in this document MUST be executed with the environment marker `MODE=system`. 

### 1.2. Mode Isolation Rule
Results from System / Kubernetes Mode are fundamentally different from **Standalone Dataplane Mode**.
*   **No Merging**: Data points from this suite MUST NOT be aggregated, averaged, or combined with Standalone results.
*   **No Standard Reporting**: This mode MUST NOT invoke `report.sh` or generate a `report.md` artifact.

### 1.3. Primary Scope
*   **Lifecycle Transitions**: Convergence time, delta latency during reloads, and state stabilization.
*   **System Integrity**: Behavior under control-plane failure and recovery.
*   **Resource Monotonicity**: Detecting leaks and hysteresis during long-running operations.

### 1.4. Non-Goals & Prohibitions
*   **No Performance Ranking**: This suite MUST NOT be used to rank proxies by performance.
*   **No Throughput Comparisons**: Throughput is a control variable used to establishment a baseline, not a comparative metric.
*   **No Micro-benchmarking**: Isolated packet processing is handled exclusively by **Standalone Dataplane Mode**.

---

## 2. Proxy Matrix

| Proxy | Role | Control Plane Component | Artifact / Protocol |
| :--- | :--- | :--- | :--- |
| **Pavis** | Primary Subject | `pavis-relay` | `.pvs` (Precompiled Binary) |
| **Envoy** | Comparative Baseline | Custom `go-control-plane` (Minimal) | xDS (LDS/RDS Only) |
| **Linkerd** | Reference System | Native Linkerd Controller | `linkerd-proxy` (Native policy) |

---

## 3. Case Catalog (Mapped to 7+2+1)

### 3.1. Config Reload Convergence
*   **Dimension**: #6 Operational Characteristics
*   **Event Trigger**: 
    *   **Pavis**: Relay `POST /v1/publish` with version $N+1$.
    *   **Envoy**: xDS Server snapshot update with new route version.
    *   **Linkerd**: `kubectl patch` ServiceProfile.
*   **Metrics**:
    *   **Convergence Time**: Duration from "trigger" to first request processed by $N+1$ config.
    *   **P99 Delta**: Latency variance during the transition window.
*   **Expectations**: Bounded convergence (target < 2s); Bounded P99 spike; Zero 5xx errors.

### 3.2. Rollback Performance
*   **Dimension**: #6 Operational Characteristics + #B Recovery & Hysteresis
*   **Event Trigger**: Inject invalid config (e.g., 100% drop), verify failure, then trigger rollback to $N-1$.
*   **Metrics**:
    *   **TTBR (Time to Baseline Restoration)**: Duration until P99 returns to steady-state baseline.
*   **Expectations**: Eventual restoration of baseline traffic within bounded timeframes.

### 3.3. Stress → Recovery
*   **Dimension**: #B Recovery & Hysteresis
*   **Event Trigger**: Apply 150% saturation load, then return to 50% baseline load.
*   **Metrics**:
    *   **Recovery Latency**: Time series of P99 post-stress.
    *   **RSS Hysteresis**: Memory delta between pre-stress and post-stabilization.
*   **Expectations**: Latency returns to baseline; RSS stabilization (no permanent growth).

### 3.4. Multi-Hour Soak (Gate Only)
*   **Dimension**: Release Gate (Durability)
*   **Event Trigger**: Steady-state load at 75% capacity for multi-hour window.
*   **Metrics**:
    *   **RSS Slope**: Monotonic growth coefficient of memory usage.
    *   **Handle Leakage**: Monitoring file descriptors and thread counts.
*   **Expectations**: RSS slope $\approx$ 0; Stable handle counts.

---

## 4. Hard Rules / Prohibited Actions

To maintain architectural and scientific integrity, the following actions are strictly prohibited:
1.  **MUST NOT** generate `report.md` or any ranking-based visualization.
2.  **MUST NOT** compare CPU, Memory, or RPS across different proxies.
3.  **MUST NOT** publish absolute timing numbers (milliseconds) from GitHub CI runs as performance claims.
4.  **MUST NOT** reuse Standalone Dataplane aggregation or summary pipelines.
5.  **MUST NOT** use System Mode results for performance marketing or competitive scoring.

---

## 5. Control-Plane Design per Proxy

### 5.1. Pavis (Relay Model)
*   **Config Version**: Defined by the PVS header `x-pavis-version`.
*   **Update Mechanism**: Relay stores PVS in memory; Runtime Agent polls via long-polling.
*   **Convergence**: When `ArcSwap` completes in the Pavis Runtime.

### 5.2. Envoy (Minimal xDS)
*   **Config Version**: xDS Snapshot Version string.
*   **Update Mechanism**: Custom `go-control-plane` implementation serving only `LDS` and `RDS`.
*   **Convergence**: Emission of Envoy `listener_added` or `route_config_updated` stats.

### 5.3. Linkerd (Native)
*   **Config Version**: Kubernetes Object Generation (`metadata.generation`).
*   **Update Mechanism**: Native Linkerd `destination` service stream.
*   **Convergence**: Observable traffic change (e.g., modified header injection).

---

## 6. Execution Profiles

The system benchmarks support two execution profiles defined by `BENCH_PROFILE`.

### 6.1. Profile: GitHub CI (`BENCH_PROFILE=github`)
*   **Goal**: Behavioral validation and recovery testing.
*   **Constraints**: Shared resources; high timing variance.
*   **Logic**: Verifies that configuration changes apply and that the system recovers from stress.
*   **Policy**: System Mode is NOT required for standard CI correctness gating.

### 6.2. Profile: Workstation (`BENCH_PROFILE=workstation`)
*   **Goal**: Threshold-based validation and trend analysis.
*   **Constraints**: Dedicated hardware; stable timing.
*   **CPU Allocation Rule**: 4 dedicated cores total: 1 for loadgen/wrk, 1 for upstream, 2 for proxy.
*   **Proxy Memory Limit**: 1GiB for proxy containers (`MEMORY_LIMIT=1G`).
*   **Logic**: Establishes high-confidence timelines for convergence and stabilization.
*   **Policy**: Primary source for internal durability audits.

---

## 7. Risks & Tradeoffs

*   **Non-Comparability**: Since control planes (xDS vs. PVS polling) are architecturally different, cross-proxy timing comparison is misleading. Evaluation is strictly per-proxy against its own baseline.
*   **Infrastructure Noise**: `kind` networking and Kubernetes API latency introduce jitter. **Tradeoff**: Thresholds are defined as "expectations" rather than hard SLAs.
*   **Environment Differences**: GitHub CI and Workstation will produce vastly different absolute numbers. **Tradeoff**: Only relative recovery and monotonic growth trends are used for cross-environment validation.

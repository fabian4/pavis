# Pavis Benchmark System

This directory contains the orchestration, configuration, and tooling for the Pavis performance evaluation suite. This system is designed to provide scientifically rigorous, reproducible measurements of proxy behavior based on the [7+2+1 Benchmark Methodology](../docs/benchmark/METHODOLOGY.md).

## 1. Dual-Mode Evaluation Model

Pavis benchmarks are split into two non-overlapping modes to isolate micro-architectural performance from macro-system behavior.

### A. Standalone Dataplane Mode
*   **Purpose**: Measure intrinsic data-plane performance (packet processing, L7 routing, and transport efficiency).
*   **Environment**: Docker-composed containers on Workstation or Bare-metal.
*   **Configuration**: Static and immutable at process startup.
*   **Scope**: Covers Core Dimensions #1–#5 and #7.
*   **Comparability**: This is the only mode permitted for cross-proxy comparisons.(Pavis vs. Envoy vs. Nginx vs. HAProxy).
*   **Output**: Generates the authoritative `report.md` and resource-cost profiles.

### B. System / Kubernetes (kind) Mode
*   **Purpose**: Measure system-level lifecycle behavior, configuration convergence, and durability.
*   **Environment**: Kubernetes (`kind`).
*   **Configuration**: Dynamic; requires active control-plane participation (Pavis Relay, Envoy xDS, or Linkerd control plane).
*   **Scope**: Covers Dimension #6 (Operational Characteristics), Derived B (Recovery), and the Durability gate.
*   **Comparability**: **Cross-proxy performance ranking is explicitly forbidden in this mode.**
*   **Output**: Event-correlated timelines and threshold-based validation (e.g., "Reload converged within 2s").

---

## 2. Execution Environments & Authority

The Pavis project maintains a strict boundary between automated regression testing and authoritative performance reporting.

### CI (GitHub-hosted Runners)
*   **Target**: Runs against **Pavis only**. Other proxies are excluded to avoid noise-polluted comparisons.
*   **Goal**: Continuous regression detection for the current branch.
*   **Constraints**:
    *   Generates a CI-only `report.md` derived solely from `summary.csv`.
    *   Does NOT produce cross-proxy rankings or comparative claims.
*   **Non-Authoritative**: Due to the shared, multi-tenant nature of GitHub-hosted runners (vCPU stealing, noisy neighbors), results are non-authoritative and used solely for internal gating.

### Workstation (Authoritative Source)
*   **Target**: All proxies in the matrix.
*   **Goal**: Generation of authoritative benchmark reports.
*   **Constraints**: Requires dedicated hardware, CPU pinning (`cpuset`), and consistent kernel tuning.
    *   **CPU Allocation Rule (Workstation)**: 4 dedicated cores total: 1 for loadgen/wrk, 1 for upstream, 2 for proxy.
    *   **Proxy Memory Limit (Workstation)**: 1GiB (`MEMORY_LIMIT=1G`).
*   **Authority**: This is the **only** environment permitted to generate:
    *   Cross-proxy performance comparisons.
    *   Payload size matrix results.
    *   Feature tax measurements (TLS, metrics overhead).
    *   Published benchmark reports for documentation.

---

### Required Execution Flags
*   `MODE=standalone` is required for `bench/run.sh`.
*   `BENCH_PROFILE=github|workstation` controls gating; `github` is Pavis-only and skips `latency_extended_1x`.
*   `BENCH_PROFILE=workstation` runs a payload matrix for `throughput_short_1x`, `latency_short_1x`, `latency_extended_1x` at `64B` and `4KiB`.
*   `REPORT_PAYLOAD_SIZE=64B|4KiB` selects a payload when generating `report.md` from matrix runs.
*   `BENCH_PAYLOAD_SIZE` defaults to `64B` (parameterized variant input).
*   `BENCH_TLS` and `BENCH_METRICS` default to `false` (variant toggles).
*   `BENCH_PROFILE=github` produces a CI-only report and must not be used for publication.

---

## 3. Case Design Philosophy

Ad-hoc or generic load tests are rejected by design. Every benchmark case script in `bench/cases/` exists exclusively to satisfy a specific dimension of the methodology:

1.  **Strict Dimension Mapping**: Each script (e.g., `throughput_short_1x.sh`) maps to a single primary engineering question (Capacity, Latency, or Stress).
2.  **Parameterized Variants**: Payload size, TLS encryption, and observability layers are treated as parameterized variants of baseline cases, not as standalone cases.
3.  **Deterministic Upstream**: All cases utilize `bench-upstream` to ensure that measured variance is attributable solely to the proxy under test.

---

## 4. Performance Integrity Disclaimer

Authoritative performance claims require hardware isolation that CI environments cannot provide. The project strictly prohibits using CI-generated metrics for public comparisons. 

All comparative rankings must be derived from the **Standalone Dataplane Mode** executed on a tuned **Workstation** with verified CPU pinning to ensure that the delta between proxies reflects architectural differences rather than environment noise.

---

## 📂 Directory Structure

*   `cases/`: Shell scripts (no embedded logic in CI systems)
*   `config/`: Static bootstrap configurations for Pavis, Envoy, Nginx, and HAProxy.
*   `scripts/`: Tools for result aggregation (`summarize.sh`) and report generation (`report.sh`).
*   `output/`: Destination for raw data, CSVs, and generated Markdown reports.

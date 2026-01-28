# Benchmark Execution Guide

This directory contains the orchestration, configuration, and tooling used to reproduce the performance evidence that backs the frozen data plane thesis. All procedures tie directly to the [7+2+1 Benchmark Methodology](../docs/benchmarks/methodology.md) and are scoped strictly to `pavis` binaries.

Side-by-side benchmarks live elsewhere; this repository only exercises Pavis itself for regression signal and thesis documentation.

## 1. Dual-Mode Evaluation Model

Pavis benchmarks are split into two non-overlapping modes to isolate micro-architectural performance from macro-system behavior.

### A. Standalone Dataplane Mode
*   **Purpose**: Measure intrinsic data-plane performance (packet processing, L7 routing, and transport efficiency).
*   **Environment**: Docker-composed containers on Workstation or Bare-metal.
*   **Configuration**: Static and immutable at process startup.
*   **Scope**: Covers Core Dimensions #1–#5 and #7.
*   **Output**: Generates the authoritative `report.md` (workstation) and resource-cost profiles. CI uses `report.github.md` for a consolidated summary.

### B. System / Kubernetes (kind) Mode
*   **Purpose**: Measure system-level lifecycle behavior, configuration convergence, and durability.
*   **Environment**: Kubernetes (`kind`) sandbox used purely for experiments; this is **not** an endorsed deployment path.
*   **Configuration**: Dynamic; requires active control-plane participation (Pavis Relay).
*   **Scope**: Covers Dimension #6 (Operational Characteristics), Derived B (Recovery), and the Durability gate.
*   **Output**: Event-correlated timelines and threshold-based validation (e.g., "Reload converged within 2s").

---

## 2. Execution Environments & Authority

The Pavis project maintains a strict boundary between automated regression testing and authoritative performance reporting.

### CI (GitHub-hosted Runners)
*   **Target**: Runs against **Pavis only**.
*   **Goal**: Continuous regression detection for the current branch.
*   **Constraints**:
    *   Generates a CI-only `report.github.md` derived from standalone + system summaries.
    *   Does NOT produce rankings or competitive claims.
*   **Non-Authoritative**: Due to the shared, multi-tenant nature of GitHub-hosted runners (vCPU stealing, noisy neighbors), results are non-authoritative and used solely for internal gating.

### Workstation (Authoritative Source)
*   **Target**: Pavis-only in this repository. Side-by-side runs live elsewhere.
*   **Goal**: Generation of authoritative benchmark reports.
*   **Constraints**: Requires dedicated hardware, CPU pinning (`cpuset`), and consistent kernel tuning.
    *   **CPU Allocation Rule (Workstation)**: 4 dedicated cores total: 1 for loadgen/wrk, 1 for upstream, 2 for proxy.
    *   **Proxy Memory Limit (Workstation)**: 1GiB (`MEMORY_LIMIT=1G`).
*   **Authority**: This is the **only** environment permitted to generate published Pavis benchmark reports for documentation.

---

### Required Execution Flags

#### Standalone Mode
*   `MODE=standalone` enables standalone dataplane mode (Docker Compose).
*   `BENCH_PROFILE=github|workstation` controls gating; `github` is Pavis-only and skips `latency_extended_1x`.
*   If `BENCH_PROFILE` is unset, it defaults to `workstation`.
*   `BENCH_PROFILE=workstation` runs a payload set for `throughput_short_1x`, `latency_short_1x`, `latency_extended_1x` at `64B` and `4KiB`.
*   `REPORT_PAYLOAD_SIZE=64B|4KiB` selects a payload when generating `report.md` from multi-size runs.
*   `BENCH_PAYLOAD_SIZE` defaults to `64B` (parameterized variant input).
*   `BENCH_TLS` and `BENCH_METRICS` default to `false` (variant toggles).
*   `BENCH_PROFILE=github` produces a CI-only `report.github.md` and must not be used for publication.

#### System Mode (Kubernetes)
*   `MODE=system` enables system/Kubernetes mode (kind cluster).
*   `BENCH_PROFILE=github` is supported in GitHub CI for system mode (CI-only gating, Pavis-only).
*   `BENCH_PROFILE=workstation` enables authoritative runs on dedicated hardware.
*   If `BENCH_PROFILE` is unset, it defaults to `workstation`.
*   `PROXY=pavis` selects the proxy to test (default: pavis).
*   System mode tests are located in `bench/cases/system/`.
*   **No MODE set**: Runs both standalone and system modes sequentially.

---

## 3. Case Design Philosophy

Ad-hoc or generic load tests are rejected by design. Every benchmark case script in `bench/cases/` exists exclusively to satisfy a specific dimension of the methodology:

1.  **Strict Dimension Mapping**: Each script (e.g., `throughput_short_1x.sh`) maps to a single primary engineering question (Capacity, Latency, or Stress).
2.  **Parameterized Variants**: Payload size, TLS encryption, and observability layers are treated as parameterized variants of baseline cases, not as standalone cases.
3.  **Deterministic Upstream**: All cases utilize `bench-upstream` to ensure that measured variance is attributable solely to the proxy under test.

---

## 4. Performance Integrity Disclaimer

Authoritative performance claims require hardware isolation that CI environments cannot provide. The project strictly prohibits using CI-generated metrics for public claims or roadmap promises.

Published reports that appear in the thesis must be derived from the **Standalone Dataplane Mode** executed on a tuned **Workstation** with verified CPU pinning.

---

## 5. System Mode Architecture

System mode tests measure operational characteristics that require a control plane and dynamic configuration updates. This mode uses Kubernetes (via `kind`) to deploy realistic service scenarios.

### Supported Proxies

| Proxy | Control Plane | Config Versioning | Sidecar Injection |
|-------|---------------|-------------------|-------------------|
| **Pavis** | pavis-relay (HTTP long-polling) | ✅ Yes | Manual (manifest) |

### System Mode Test Cases

Located in `bench/cases/system/`:

1. **config_reload_convergence.sh**
   - Measures time from config publish to first request served by new version
   - Validates zero 5xx errors during transition
   - Measures P99 latency delta during convergence

2. **rollback_performance.sh**
   - Tests rollback from a degraded (fixed delay) config to known-good baseline
   - Proves apply/rollback via response fingerprints before timing TTBR
   - Measures TTBR (Time to Baseline Restoration) once v1 is re-applied and latency recovers

3. **stress_recovery.sh**
   - Applies 150% saturation load, then returns to baseline
   - Measures latency recovery timeline
   - Checks RSS hysteresis (memory growth)
   - Pavis only

4. **multi_hour_soak.sh**
   - Runs at 75% capacity for 4+ hours
   - Tracks RSS slope via linear regression
   - Validates file descriptor stability
   - Pavis only

### Architecture Components

**Standalone Mode** (Docker Compose):
```
┌─────────────┐    ┌─────────┐    ┌──────────┐
│ bench-      │───▶│  Proxy  │───▶│ bench-   │
│ loadgen     │    │ (sidecar│    │ upstream │
└─────────────┘    └─────────┘    └──────────┘
```

**System Mode** (Kubernetes):
```
┌─────────────────────────────────────────────┐
│ kind cluster (pavis-bench)                  │
│                                             │
│  ┌──────────────┐                           │
│  │ Control      │  /v1/config               │
│  │ Plane        │◀────────────┐             │
│  │ (relay)      │             │             │
│  └──────────────┘             │             │
│                               │             │
│  ┌────────────────────────────┴───────────┐ │
│  │ test-backend Pod                       │ │
│  │  ┌──────────┐   ┌──────────────────┐   │ │
│  │  │  Proxy   │──▶│  bench-upstream  │   │ │
│  │  │ (sidecar)│   │  (container)     │   │ │
│  │  └──────────┘   └──────────────────┘   │ │
│  └────────────────────────────────────────┘ │
│         ▲                                   │
│         │ port-forward                      │
└─────────┼───────────────────────────────────┘
          │
   ┌──────┴───────┐
   │ bench-loadgen│ (host)
   └──────────────┘
```

## 6. Quick Start

### Standalone Mode (Dataplane Performance)

```bash
# Build required binaries and images
make bench-standalone-build

# Run all standalone tests with Pavis (workstation profile)
MODE=standalone BENCH_PROFILE=workstation \
  BACKEND_CPUSET=0 PROXY_CPUSET=1-2 BENCH_LOADGEN_CPUSET=3 \
  make bench-standalone

# Run specific test case
MODE=standalone CASE="throughput_short_1x" make bench-standalone

# Cleanup
make bench-standalone-down

# Generate report
make bench-report

# Backward compatibility aliases (deprecated, use explicit targets above)
# make bench-build, make bench, make bench-all, make bench-down
```

### System Mode (Operational Characteristics)

```bash
# Build system mode images
make bench-system-build

# Run Pavis system tests
MODE=system PROXY=pavis BENCH_PROFILE=workstation \
  make bench-system

# Cleanup kind cluster
make bench-system-down
```

### Both Modes

```bash
# Run both standalone and system modes sequentially
# Option 1: Using explicit standalone target
BENCH_PROFILE=workstation \
  make bench-standalone

BENCH_PROFILE=workstation \
  make bench-system

# Option 2: Let bench/run.sh handle both modes (when MODE is unset)
BENCH_PROFILE=workstation \
  bash bench/run.sh
```

## 7. Environment Variables Reference

| Variable | Values | Default | Description |
|----------|--------|---------|-------------|
| `MODE` | `standalone`, `system`, unset | unset | Benchmark mode (unset runs both) |
| `BENCH_PROFILE` | `github`, `workstation` | `workstation` | Execution environment |
| `PROXY` | `pavis` | `pavis` | Proxy under test |
| `CASE` | case names (space-separated) | all | Specific test cases to run |
| `BENCH_PAYLOAD_SIZE` | `64B`, `4KiB`, etc. | `64B` | Request/response payload size |
| `BENCH_TLS` | `true`, `false` | `false` | Enable TLS encryption |
| `BENCH_METRICS` | `true`, `false` | `false` | Enable Prometheus metrics |
| `BACKEND_CPUSET` | CPU list (e.g., `0`) | auto-detect | CPU affinity for upstream |
| `PROXY_CPUSET` | CPU list (e.g., `1-2`) | auto-detect | CPU affinity for proxy |
| `BENCH_LOADGEN_CPUSET` | CPU list (e.g., `3`) | auto-detect | CPU affinity for loadgen |
| `DRY_RUN` | `1`, `0` | `0` | Validate setup without running tests |

## 📂 Directory Structure

*   `cases/`: Test case scripts
    *   `cases/standalone/`: Standalone mode test scripts (dataplane performance)
    *   `cases/system/`: System mode test scripts (config convergence, soak tests)
*   `config/`: Static bootstrap configurations for Pavis
*   `k8s/`: Kubernetes manifests for system mode
    *   `k8s/pavis/`: Pavis relay deployment and test workloads
*   `scripts/`: Tools for setup, execution, metrics, and reporting
    *   `scripts/setup.sh`: Unified environment setup (standalone + system)
    *   `scripts/k8s_helpers.sh`: Kubernetes utility functions
*   `scripts/proxy_helpers.sh`: Pavis test abstractions
*   `scripts/summarize_github.sh`: Result aggregation
    *   `scripts/report.sh`: Report generation
*   `output/`: Destination for raw data, CSVs, and generated Markdown reports
    *   `output/standalone/`: Standalone mode results
    *   `output/system/`: System mode results

# Benchmark Case Script Review

- **Review type**: Benchmark Case Review
- **Target**: bench/cases/*.sh
- **Generation timestamp**: 2026-01-13T16:30:00Z
- **AI model identifier**: Gemini 2.0 Flash

## 1. Executive Summary

The Pavis benchmark suite consists of 6 core cases designed to measure capacity, latency, stability, efficiency, and operational impact. Overall, the **cases are partially sound with a significant semantic gap in operational testing**. 

- **Capacity & Latency**: These cases are well-implemented, utilizing both closed-loop (`wrk`) and open-loop (`bench-loadgen`) models appropriately.
- **Stress & Churn**: Correctly leverage `wrk` with high connection counts or explicit `Connection: close` headers to stress the proxy's transport layer.
- **Operational (Reload)**: This is currently a **placeholder**; it measures open-loop latency but fails to trigger or validate any configuration reload events.

## 2. Per-Case Review

### churn_short_1x.sh
- **Stated Intent**: Measure performance under connection churn.
- **Observed Behavior**: Closed-loop (`wrk`) with `Connection: close` header forced on every request.
- **Alignment Verdict**: **Aligned**
- **Key Evidence**: `churn_short_1x.sh:164` (`local header_args=(-H "Connection: close")`)

### concurrency_short_1x.sh
- **Stated Intent**: Verify capacity under high connection counts.
- **Observed Behavior**: Closed-loop (`wrk`) with 5000 concurrent connections.
- **Alignment Verdict**: **Aligned**
- **Key Evidence**: `concurrency_short_1x.sh:17` (`CONNECTIONS=5000`)

### latency_extended_1x.sh
- **Stated Intent**: Measure stability and capture outliers over a long duration.
- **Observed Behavior**: Open-loop (`bench-loadgen`) at 10k RPS for 300 seconds, repeated 5 times.
- **Alignment Verdict**: **Aligned**
- **Key Evidence**: `latency_extended_1x.sh:14-20` (`DURATION_S=300`, `RUN_COUNT=5`)

### latency_short_1x.sh
- **Stated Intent**: Standard baseline latency measurement.
- **Observed Behavior**: Open-loop (`bench-loadgen`) at 10k RPS for 30 seconds.
- **Alignment Verdict**: **Aligned**
- **Key Evidence**: `latency_short_1x.sh:14-20` (`DURATION_S=30`, `TARGET_RPS=10000`)

### reload_short_1x.sh
- **Stated Intent**: Measure latency impact of configuration reloads.
- **Observed Behavior**: Open-loop (`bench-loadgen`) at 5k RPS. **No reload is triggered**.
- **Alignment Verdict**: **Misaligned / Placeholder**
- **Key Evidence**: `reload_short_1x.sh:11` (`# Reload triggering is not implemented; this runs as a normal open-loop latency test.`), `reload_short_1x.sh:22` (`PLACEHOLDER=true`)

### throughput_short_1x.sh
- **Stated Intent**: Measure absolute maximum request forwarding rate.
- **Observed Behavior**: Closed-loop (`wrk`) with 100 connections, repeated 5 times to find maximum throughput.
- **Alignment Verdict**: **Aligned**
- **Key Evidence**: `throughput_short_1x.sh:14-19` (`LOAD_TYPE="closed-loop"`, `RUN_COUNT=5`)

## 3. Cross-Case Findings

- **Boilerplate Duplication**: All scripts share a common template for Docker orchestration, health checks, and stats collection. While consistent, this leads to large scripts (200+ lines) where only ~5 lines define the actual workload.
- **Resource Pinning**: All cases assume `cpuset` availability and pin backend to CPU 0 and proxy to CPU 1-2. This is consistent across the suite.
- **Error Attribution**: Most scripts correctly parse `wrk` socket errors and `bench-loadgen` dropped requests, providing a clear signal of saturation.

## 4. Risk Assessment

- **Safe Conclusions**: 
  - Absolute throughput (RPS) comparisons.
  - Tail latency (P99) under steady-state load.
  - Per-connection memory overhead (RSS) in concurrency tests.
- **Unsupported/Misleading Conclusions**:
  - **Operational Impact**: Any results from `reload_short_1x.sh` represent only baseline performance at 5k RPS, **not** the cost of a reload. Claiming "Pavis has zero-impact reloads" based on this case would be false.

## 5. Final Verdict

**Benchmark Cases Have Limited Decision Value**

(Specifically: Capacity and Latency cases are high-value, but the Operational dimension is entirely unverified.)

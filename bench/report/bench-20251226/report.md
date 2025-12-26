# Benchmark Report: Pavis Performance Analysis
**Date:** 2025-12-26 | 
**Version:** `bench-20251226` (`37c678ee82cfa79c844bc3ba00e4d4af28b251f6`)

> Proxies: `envoy(v1.32.2)` · `haproxy(2.9.6-alpine)` · `nginx(1.26.2-alpine)` · `pavis(bench-20251226)`  
> Workloads: `churn` · `concurrency` · `latency` · `throughput`  
> Profiles: `baseline` · `cpu-limited` · `memory-limited`
> 
> Run at: https://github.com/fabian4/pavis/actions/runs/20516504677
---

Pavis demonstrates exceptional **memory efficiency** even under high stress, though it currently faces a **concurrency bottleneck** that limits its scaling potential compared to mature C++ or C based proxies.

## Key Findings

Focus: The following findings highlight areas where Pavis demonstrates clear advantages over industry proxies, as well as architectural gaps that currently limit scalability.

### 1. Notable Memory Efficiency 🏆

Pavis shows improved memory efficiency under throughput-oriented workloads, using less average and peak memory than several other evaluated proxies in the baseline profile.

* **Insight:** Pavis maintains a consistently low memory footprint during sustained throughput tests
*   **Evidence:** *Throughput (baseline, 30s)*
    *   **Pavis:** **11.24 MiB (avg) / 11.75 MiB (peak)**
    *   **Envoy:** 24.65 MiB / 24.88 MiB
    *   **HAProxy:** 12.17 MiB / 13.23 MiB
    *   **Nginx:** 12.48 MiB / 12.75 MiB
---

### 2. Throughput Performance Gap 📉
Pavis trails baseline throughput by approximately **12–15%**. While processing fewer requests per second, it maintains comparable tail latency to Envoy and Nginx.

*   **Insight:** Core request processing logic is stable but requires optimization for total volume.
*   **Evidence:** *Throughput (baseline, 100 connections, 30s)*
    *   **Pavis:** 1034.87 RPS
    *   **Envoy:** 1173.05 RPS (**-11.8%** gap)

---

### 3. High Concurrency Scaling Bottleneck 🚧
Pavis does not scale efficiently as concurrent connections increase. Saturation occurs early, with limited throughput growth when moving from 5k to 10k connections.

*   **Insight:** Throughput does not increase proportionally with connection count, indicating early saturation in connection handling or scheduling.
*   **Evidence:** *RPS Comparison*
    *   **5k Connections:** Pavis 964.85 vs Envoy 5916.14
    *   **10k Connections:** Pavis 885.85 vs Envoy 5829.63

---

## Cause Analysis:

### Primary Failure Mode: Missing Connection Pooling 🔍
High-concurrency failures are dominated by connection lifecycle errors rather than application logic.

*   **Symptom:** As downstream concurrency reaches 5k-10k, Pavis attempts to open an equal number of upstream connections.
*   **Result:** This flood of new connections exhausts the backend's accept queue or worker limits, causing it to reset connections.
*   **Evidence:** 
    *   Logs show frequent `Upstream ReadError: Connection reset by peer (os error 104)` occurring *while reading response headers* (0 bytes read).
    *   Logs show `Downstream ConnectionClosed` occurring *prematurely before response header*, indicating the proxy was too slow to establish the upstream link.

---

## Next Steps

1.  **Implement Connection Pooling (Critical):** Transition from transient `HttpPeer` connections to a persistent connection pool. This is expected to eliminate the 1:1 connection bottleneck, reduce handshake overhead, and prevent backend exhaustion.
2.  **Runtime Tuning:** Optimize the async task scheduling for high-concurrency connection acceptance.
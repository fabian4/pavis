# Pavis Benchmark

Performance comparison of Pavis against industry-standard proxies (Envoy, Nginx, HAProxy) using orthogonal benchmark dimensions.

## Overview

The benchmark measures HTTP data-plane throughput, latency, and resource efficiency under various constraints.

**Architecture:**
```
wrk (host) ──▶ Proxy (Docker) ──▶ httpbin (Docker)
```

## Quick Start

```bash
# Prerequisites: wrk, bc, docker
make benchmark        # Run full matrix (44 runs)
make benchmark-down   # Cleanup and remove containers
```

**Results:**
- **Report:** `output/summary.md` (Human readable)
- **Metrics:** `output/results.csv` (Raw data)
- **Logs:** `output/{proxy}.txt` (Full tool output)

## Benchmark Matrix

Total of 44 runs (11 configurations × 4 proxies).

| Dimension | Values |
|-----------|--------|
| **Workload** | `throughput` (100 conn), `latency` (500), `concurrency` (5k, 10k), `churn` (100) |
| **Resource** | `baseline` (2 CPU, 512M), `cpu-limited` (1 CPU), `memory-limited` (256M) |
| **Duration** | `short` (30s), `extended` (300s) |
| **Intensity** | `1x` (default), `2x` (multiplied connections) |

### Test Scenarios
- **CI Matrix:** All workloads under baseline resources (short duration).
- **Resource Analysis:** Throughput and Churn under CPU/Memory limits.
- **Stability:** Extended 5-minute runs for throughput and latency.
- **Stress:** Latency and Concurrency at 2x intensity.

## Methodology

- **Tooling:** `wrk` with 4 threads; `docker stats` for resource tracking.
- **Isolation:** Each run uses a fresh container with strict Docker cgroup limits.
- **Warmup:** 5s warmup period excluded from final measurements.
- **Consistency:** All proxies use 2 worker threads, HTTP/1.1, and disabled logging to ensure architectural differences are measured rather than configuration tuning.

## File Structure

```
bench/
├── config/       # Proxy-specific configurations
├── output/       # Generated output (ignored by git)
├── scripts/      # run.sh, csv.sh, summary.sh
├── bench.yaml    # Matrix specification (reference)
└── docker-compose.yaml
```
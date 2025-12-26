# Pavis Benchmark

Performance comparison of Pavis against industry-standard proxies.

## Architecture

```
┌─────────────┐      ┌─────────────────────┐      ┌──────────────────┐
│  wrk (host) │ ───▶ │  Proxy (container)  │ ───▶ │ httpbin (backend)│
│  4 threads  │      │  cgroup-limited     │      │  container       │
└─────────────┘      └─────────────────────┘      └──────────────────┘
```

**Proxies:**

| Proxy | Language | Port | Description |
|-------|----------|:----:|-------------|
| Pavis | Rust | 8080 | This project (async, Pingora-based) |
| Envoy | C++ | 8081 | Industry standard service proxy |
| Nginx | C | 8082 | Widely-used reverse proxy |
| HAProxy | C | 8083 | Mature, highly-optimized proxy |

## Quick Start

**GitHub Actions:**
Benchmarks run manually via [GitHub Actions CI](https://github.com/fabian4/pavis/actions/workflows/bench.yaml).

**Local:**

```bash
# Prerequisites: wrk (or wrk2), docker, bc
make benchmark        # Run full matrix (44 runs)
make benchmark-down   # Cleanup containers
```

**Output:**

| Path | Description                     |
|------|---------------------------------|
| `output/{proxy}/{proxy}.txt` | Raw wrk output + resource stats |
| `output/{proxy}/logs/*.log` | Proxy logs per test run         |
| `output/results.csv` | Aggregated metrics              |
| `output/summary.md` | Extracted report                |

## Benchmark Matrix

**44 total runs** = 11 configurations × 4 proxies

### Dimensions

| Dimension | Values | Description |
|-----------|--------|-------------|
| **Workload** | throughput, latency, concurrency, churn | Operational pattern |
| **Resource** | baseline, cpu-limited, memory-limited | Container cgroup limits |
| **Duration** | short (30s), extended (300s) | Measurement window |
| **Intensity** | 1x, 2x | Connection count multiplier |

### Workloads

| Workload | Connections | Description |
|----------|:-----------:|-------------|
| throughput | 100 | RPS under light load |
| latency | 500 | Tail latency under sustained load |
| concurrency | 5,000 | High concurrent connection stress |
| churn | 100 | Rapid connect/disconnect (`Connection: close`) |

### Resource Profiles

| Profile | CPU | Memory | Purpose |
|---------|:---:|:------:|---------|
| baseline | 2 cores | 512 MiB | Normal operating conditions |
| cpu-limited | 1 core | 512 MiB | CPU saturation behavior |
| memory-limited | 2 cores | 256 MiB | Memory pressure behavior |

### Test Matrix

#### CI Matrix (4 runs)

| # | Config ID | Workload | Resource | Duration | Intensity |
|:-:|-----------|----------|----------|:--------:|:---------:|
| 1 | `throughput_baseline_short_1x` | throughput | baseline | 30s | 1x |
| 2 | `latency_baseline_short_1x` | latency | baseline | 30s | 1x |
| 3 | `concurrency_baseline_short_1x` | concurrency | baseline | 30s | 1x |
| 4 | `churn_baseline_short_1x` | churn | baseline | 30s | 1x |

#### Extended Matrix (7 runs)

| # | Config ID | Workload | Resource | Duration | Intensity | Purpose |
|:-:|-----------|----------|----------|:--------:|:---------:|---------|
| 5 | `throughput_cpu-limited_short_1x` | throughput | cpu-limited | 30s | 1x | CPU saturation |
| 6 | `churn_cpu-limited_short_1x` | churn | cpu-limited | 30s | 1x | Handshake under CPU limit |
| 7 | `throughput_memory-limited_short_1x` | throughput | memory-limited | 30s | 1x | Memory pressure |
| 8 | `throughput_baseline_extended_1x` | throughput | baseline | 300s | 1x | Steady-state stability |
| 9 | `latency_baseline_extended_1x` | latency | baseline | 300s | 1x | Long-term latency |
| 10 | `latency_baseline_short_2x` | latency | baseline | 30s | 2x | 1000 conn latency |
| 11 | `concurrency_baseline_short_2x` | concurrency | baseline | 30s | 2x | 10k connection stress |

## Methodology

| Aspect | Details |
|--------|---------|
| Load Generator | `wrk` (or `wrk2`) with 4 threads |
| Resource Tracking | `docker stats` sampled every 1s |
| Isolation | Fresh container per resource profile |
| Warmup | 5s excluded from measurements |
| Consistency | All proxies: 2 workers, HTTP/1.1, logging disabled |
| Backend | `httpbin` `/get` endpoint |

## File Structure

```
bench/
├── BENCHMARKS.md           # Report index
├── README.md               # This file
├── bench.yaml              # Matrix specification
├── docker-compose.yaml     # Container definitions
├── config/                 # Proxy configurations
│   ├── envoy.yaml
│   ├── haproxy.cfg
│   ├── nginx.conf
│   └── pavis.yaml
├── scripts/
│   ├── run.sh              # Main runner
│   ├── csv.sh              # CSV aggregation
│   └── summary.sh          # Report generation
└── report/                 # Archived reports
    └── bench-YYYYMMDD/
        └── report.md
```

## Reports

See **[BENCHMARKS.md](./BENCHMARKS.md)** for the index of all benchmark reports.
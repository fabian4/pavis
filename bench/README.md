# Pavis Benchmark

**Version 2.0** - Enhanced with methodological improvements for credible, reproducible performance comparison.

Performance comparison of Pavis against industry-standard proxies with focus on:
- **Open-loop latency testing** (wrk2) to avoid coordinated omission
- **Backend bottleneck elimination** (minimal backend option)
- **Statistical validation** (multi-run with median/IQR)
- **CPU isolation** (pinned cores for proxy, backend, load generator)
- **Configuration fairness** (documented semantic equivalence)

📖 **Full Methodology**: See [METHODOLOGY.md](./METHODOLOGY.md)
⚖️ **Fairness Checklist**: See [FAIRNESS.md](./FAIRNESS.md)

## Architecture

```
┌─────────────┐      ┌─────────────────────┐      ┌──────────────────┐
│  wrk/wrk2   │ ───▶ │  Proxy (container)  │ ───▶ │ Backend          │
│  (host)     │      │  CPU: 1-2           │      │ (httpbin/minimal)│
│  4 threads  │      │  cgroup-limited     │      │ CPU: 0           │
└─────────────┘      └─────────────────────┘      └──────────────────┘
                            ↓                              ↓
                       Pinned CPUs                   Pinned CPU
                       (isolation)                   (isolation)
```

**Load Generators:**
- **wrk2** (open-loop): Latency workloads with fixed target RPS (avoids coordinated omission)
- **wrk** (closed-loop): Throughput, concurrency, churn workloads (maximum RPS testing)

**Proxies:**

| Proxy | Language | Port | Description |
|-------|----------|:----:|-------------|
| Pavis | Rust | 8080 | This project (async, Pingora-based) |
| Envoy | C++ | 8081 | Industry standard service proxy |
| Nginx | C | 8082 | Widely-used reverse proxy |
| HAProxy | C | 8083 | Mature, highly-optimized proxy |

**Backends:**

| Backend | Type | Description | Use Case |
|---------|------|-------------|----------|
| httpbin | Functional | kennethreitz/httpbin | Realistic application behavior |
| minimal | Dataplane | Lightweight Go server | Proxy dataplane isolation |

## Quick Start

**GitHub Actions:**
Benchmarks run manually via [GitHub Actions CI](https://github.com/fabian4/pavis/actions/workflows/bench.yaml).

**Local (Standard):**

```bash
# Prerequisites: wrk (or wrk2), docker, bc
make benchmark        # Run full matrix (44 runs)
make benchmark-down   # Cleanup containers
```

**Local (with wrk2 for open-loop latency):**

```bash
# Install wrk2 (open-loop load generator)
# macOS:
brew tap jabley/homebrew-wrk2
brew install wrk2

# Ubuntu:
git clone https://github.com/giltene/wrk2.git
cd wrk2 && make && sudo cp wrk2 /usr/local/bin/

# Run benchmarks
make benchmark
```

**Advanced Options:**

```bash
# Use minimal backend for dataplane isolation
BACKEND_TYPE=minimal make benchmark

# Multi-run mode (N=5 iterations)
BENCHMARK_RUNS=5 make benchmark

# CPU performance governor (recommended)
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

**Output:**

| Path | Description                     |
|------|---------------------------------|
| `output/{proxy}/{proxy}.txt` | Raw wrk output + resource stats |
| `output/{proxy}/logs/*.log` | Proxy logs per test run         |
| `output/results.csv` | Aggregated metrics              |
| `output/summary.md` | Extracted report                |

## Benchmark Matrix

**46 total runs** = (11 configurations × 4 proxies) + 2 Pavis-specific

### Dimensions

| Dimension | Values | Description |
|-----------|--------|-------------|
| **Workload** | throughput, latency, concurrency, churn, reload† | Operational pattern |
| **Load Type** | open-loop, closed-loop | Load generation strategy |
| **Resource** | baseline, cpu-limited, memory-limited | Container cgroup limits |
| **Duration** | short (30s), extended (300s) | Measurement window |
| **Intensity** | 1x, 2x | Connection count multiplier |
| **Backend** | httpbin, minimal | Backend service type |
| **Runs** | single (N=1), multi (N=5) | Statistical validation |

† Pavis-specific workloads (reload, config-scale) not run for other proxies

### Workloads

| Workload | Connections | Load Type | Target RPS | Description |
|----------|:-----------:|-----------|:----------:|-------------|
| throughput | 100 | closed-loop | - | RPS under light load |
| latency | 500 | **open-loop** | 10,000 | Tail latency under sustained load |
| concurrency | 5,000 | closed-loop | - | High concurrent connection stress |
| churn | 100 | closed-loop | - | Rapid connect/disconnect (`Connection: close`) |
| reload† | 500 | **open-loop** | 5,000 | Hot-reload latency jitter (Pavis frozen dataplane) |

**Open-loop workloads** use wrk2 with fixed target RPS to avoid coordinated omission.

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
├── METHODOLOGY.md           # Full methodology documentation (NEW)
├── FAIRNESS.md              # Proxy configuration fairness checklist (NEW)
├── README.md                # This file
├── bench.yaml               # Matrix specification (enhanced)
├── docker-compose.yaml      # Container definitions (enhanced with CPU pinning)
├── backend/                 # Minimal backend server (NEW)
│   ├── Dockerfile
│   └── minimal-server.go
├── config/                  # Proxy configurations
│   ├── envoy.yaml
│   ├── haproxy.cfg
│   ├── nginx.conf
│   └── pavis.yaml
├── scripts/
│   ├── run.sh               # Main runner (enhanced: wrk2, multi-run, backend selection)
│   ├── csv.sh               # CSV aggregation (enhanced: multi-run stats, new metrics)
│   └── summary.sh           # Report generation
└── report/                  # Archived reports
    └── bench-YYYYMMDD/
        └── report.md
```

## Reports

See **[BENCHMARKS.md](./BENCHMARKS.md)** for the index of all benchmark reports.

---

## Limitations & Known Issues

1. **wrk2 Installation**: Open-loop latency tests require wrk2 (not installed by default on most systems)
2. **Reload Benchmark**: Hot-reload triggering mechanism not yet implemented (placeholder test)
3. **CPU Governor**: Must be manually set to `performance` for stable results
4. **Single-Host**: All containers run on same host (no multi-node distributed testing)
5. **HTTP/1.1 Only**: Current tests do not cover HTTP/2 or gRPC

See [METHODOLOGY.md](./METHODOLOGY.md#limitations--known-issues) for full details.

---

## Changelog

### Version 2.0 (2026-01-09)
- **Added**: wrk2 open-loop latency testing
- **Added**: Minimal backend server for dataplane isolation
- **Added**: Multi-run statistical validation (N=5 runs)
- **Added**: CPU pinning for resource isolation
- **Added**: Backend saturation detection
- **Added**: METHODOLOGY.md and FAIRNESS.md documentation
- **Enhanced**: CSV output with load_type, backend_type, median/IQR metrics
- **Enhanced**: bench.yaml with Pavis-specific benchmarks (reload, config-scale)

### Version 1.0 (Initial)
- Basic benchmark matrix with wrk
- 4 proxies × 11 configurations = 44 runs
- httpbin backend only
- Single-run results
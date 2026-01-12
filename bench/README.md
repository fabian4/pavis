# Pavis Benchmark Suite

The Pavis Benchmark Suite is a comprehensive performance testing framework designed to compare Pavis against industry-standard proxies (Envoy, Nginx, HAProxy) under scientifically rigorous conditions.

## 📚 Documentation Structure

- **[README.md](./README.md)**: Overview, Quickstart, and Usage Guide (this file).
- **[Methodology](../docs/benchmark/METHODOLOGY.md)**: 7-dimension framework for proxy evaluation.
- **[Benchmark Cases](../docs/benchmark/CASES.md)**: Concrete test case definitions and coverage mapping.
- **[FAIRNESS.md](../docs/benchmark/FAIRNESS.md)**: Proxy configuration comparison and fairness guarantees.

---

## 🚀 Quick Start

### 1. Prerequisites
- `docker` and `docker-compose`
- `jq` (for JSON parsing)
- `bc` (for statistics)
- Linux environment recommended for best results (CPU pinning support).

### 2. Build Images
Build the required images before running benchmarks:

```bash
# Build bench-upstream (canonical backend)
make docker-build IMAGE=bench-upstream

# Build pavis proxy
make docker-build IMAGE=pavis
```

### 3. Run Benchmarks

**Fastest Validation (Dry-Run):**
```bash
# Validate setup without running actual loads (~20s)
DRY_RUN=1 make bench
```

**Run Single Test Case:**
```bash
# Run throughput test on Pavis (~1m)
make bench CASE="throughput_short_1x"
```

**Run All Default Cases:**
```bash
# Run all 6 default test cases (~15-20m)
make bench
```

**Compare Proxies:**
```bash
# Test Nginx instead of Pavis
make bench PROXY=nginx CASE="throughput_short_1x"
```

### 4. View Results
Results are stored in `bench/output/`.

```bash
# View summary of the latest run
cat bench/output/pavis/throughput_short_1x/*/summary.json | jq .

# View aggregated results index
cat bench/output/pavis/index_*.csv
```

---

## ⚙️ Usage Guide

### Environment Variables

Control the benchmark behavior with these variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `PROXY` | `pavis` | Target proxy: `pavis`, `envoy`, `nginx`, `haproxy`. |
| `CASE` | (all) | Space-separated list of test cases to run. |
| `DRY_RUN` | `0` | Set to `1` to skip load generation (validate setup only). |
| `BENCH_VERBOSE` | `0` | Set to `1` for full Docker/tool output. |
| `LOADGEN_WARN` | `0` | Set to `1` to show load generator warnings (stderr). |
| `ENVOY_TAG` | `v1.32.2` | Override Envoy Docker image tag. |
| `NGINX_TAG` | `1.26.2-alpine` | Override Nginx Docker image tag. |

### Advanced Examples

**Debug Mode:**
Run a single case with verbose output to troubleshoot.
```bash
BENCH_VERBOSE=1 LOADGEN_WARN=1 CASE=latency_short_1x make bench
```

**Custom Proxy Version:**
Test a specific version of Envoy.
```bash
ENVOY_TAG=v1.33.0 make bench PROXY=envoy
```

**Statistical Validation:**
Run the full benchmark suite with multiple iterations (N=5).
```bash
BENCHMARK_RUNS=5 make benchmark
```

---

## 📂 Project Structure

```
bench/
├── README.md              # This guide
├── FAIRNESS.md            # Config comparison & Fairness checklist
├── run.sh                 # Main benchmark runner script
├── docker-compose.yaml    # Container definitions
├── config/                # Proxy configurations
│   ├── pavis.yaml
│   ├── envoy.yaml
│   ├── nginx.conf
│   └── haproxy.cfg
├── cases/                 # Individual test case scripts
│   ├── throughput_short_1x.sh
│   ├── latency_short_1x.sh
│   └── ...
├── scripts/               # Helper scripts (reporting, summarizing)
└── output/                # Benchmark results
```

---

## 🔧 Troubleshooting

### Common Issues

**`error: bench-loadgen not found`**
The latency tests require `bench-loadgen`. It is built automatically by `make bench`. You can also build it manually:
```bash
cargo build -p pavis-benchkit --bin bench-loadgen --release
```

**`backend failed to become healthy`**
Check the backend logs:
```bash
docker logs bench-upstream
```

**Results show high variance / instability**
- Ensure you are running on **Linux**. macOS does not support CPU pinning (`cpuset`).
- Set the CPU governor to `performance`:
  ```bash
  echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
  ```
- Close other resource-intensive applications.

**Permission denied errors**
Increase your file descriptor limit:
```bash
ulimit -n 65535
```

---

## 📊 Results Analysis

**Per-Run Output (`bench/output/{proxy}/{case}/`)**
- `summary.json`: Parsed metrics (RPS, Latency P99, etc.).
- `wrk.txt` / `loadgen.txt`: Raw tool output.
- `docker_stats.csv`: Container resource usage during the run.

**Aggregated Reports**
Generate a summary report from all runs:
```bash
bash bench/scripts/report.sh
```

The summary CSV (`bench/output/summary.csv`) contains key metrics for all iterations and can be imported into spreadsheet tools for analysis.
# Pavis Benchmark

Performance comparison of Pavis against industry-standard proxies with focus on:
- **Open-loop latency testing** (wrk2) to avoid coordinated omission.
- **Backend bottleneck elimination** (bench-upstream only).
- **Statistical validation** (multi-run with median/IQR).
- **CPU isolation** (pinned cores for proxy, backend, load generator).
- **Configuration fairness** (documented semantic equivalence).

📖 **Detailed References**
- **[QUICKSTART.md](./QUICKSTART.md)**: Quick reference guide with common commands.
- **[METHODOLOGY.md](./METHODOLOGY.md)**: Scientific foundations, metric definitions, and the full test matrix.
- **[FAIRNESS.md](./FAIRNESS.md)**: Detailed proxy configuration parity checklist.

---

## 🚀 Quick Start

### 1. Prerequisites
- `wrk` (for throughput/concurrency/churn tests)
- `docker` and `docker-compose`
- `jq` (for JSON parsing)
- `bc` (for statistics)
- `ulimit -n 10000` (recommended)

**Note**: `wrk2` is no longer required. Latency tests use `bench-loadgen`, which is automatically built from `crates/pavis-benchkit/src/bin/bench-loadgen.rs` when running `make bench`.

### 2. Build Docker Images

Build the required images before running benchmarks:

```bash
# Build bench-upstream (canonical backend)
make docker-build IMAGE=bench-upstream

# Build pavis proxy
make docker-build IMAGE=pavis

# Optional: Build other proxies if testing them
# (Note: nginx, envoy, haproxy use official images with default tags)
```

### 3. Quick Test Commands

**Quick Validation (Dry-Run, ~20 seconds):**
```bash
# Validate setup without running benchmarks
DRY_RUN=1 make bench
```

**Single Test Case (~1 minute):**
```bash
# Run single benchmark case
make bench CASE="throughput_short_1x"
```

**Multiple Test Cases (~3-5 minutes):**
```bash
# Run specific cases
make bench CASE="throughput_short_1x latency_short_1x"
```

**All Default Cases (~15-20 minutes):**
```bash
# Run all 6 default test cases
make bench
```

**Test Different Proxy:**
```bash
# Test nginx instead of pavis
make bench PROXY=nginx CASE="throughput_short_1x"
```

### 4. Advanced: Full Statistical Validation

**Full Matrix (All proxies, all cases, ~45 mins):**
```bash
make benchmark
```

**Statistical Validation (N=5 iterations):**
```bash
BENCHMARK_RUNS=5 make benchmark
```

---

## 🎯 Test Cases

The benchmark suite includes 6 test cases in `bench/cases/`:

| Case | Load Type | Duration | Tool | Focus |
|------|-----------|----------|------|-------|
| `throughput_short_1x` | Closed-loop | 30s | wrk | Maximum RPS |
| `latency_short_1x` | Open-loop | 30s | bench-loadgen | Latency distribution |
| `latency_extended_1x` | Open-loop | 300s | bench-loadgen | Tail latency stability |
| `concurrency_short_1x` | Closed-loop | 30s | wrk | High connection count |
| `churn_short_1x` | Closed-loop | 30s | wrk | Connection churn |
| `reload_short_1x` | Open-loop | 30s | bench-loadgen | Config reload impact |

---

## ⚙️ Configuration

### Environment Variables

```bash
# Target proxy (default: pavis)
PROXY=<pavis|envoy|nginx|haproxy>

# Test cases to run (default: all 6 cases)
CASE="throughput_short_1x latency_short_1x ..."

# Dry-run mode - validate setup without benchmarks (default: off)
DRY_RUN=1

# Verbose output mode - show full Docker and tool output (default: 0 for compact)
BENCH_VERBOSE=<0|1>

# Loadgen warning output - show bench-loadgen stderr warnings (default: 0 for quiet)
LOADGEN_WARN=<0|1>
```

**Output Verbosity Control:**

- **`BENCH_VERBOSE=0` (default)**: Compact output for CI/automated runs
  - Suppresses Docker compose startup logs
  - Compact CPU pinning validation (`cpuset_pavis=1-2 expected=1-2 ok`)
  - Compact backend health check (`backend_ready=ok`)
  - Tool parameter summary (`tool=loadgen duration=30s connections=500 target_rps=10000`)
  - One-line results summary (`Results: rps=9673 p50=0.7ms p99=1.2ms errors=0`)

- **`BENCH_VERBOSE=1`**: Full verbose output for debugging
  - Docker compose startup logs
  - Full wrk/bench-loadgen output
  - Detailed cpuset validation messages
  - Run-by-run progress for multi-run tests

- **`LOADGEN_WARN=0` (default)**: Suppress bench-loadgen stderr warnings
  - Quiets rate limiter warnings, dropped request notices, etc.
  - Cleaner output for CI logs

- **`LOADGEN_WARN=1`**: Show all bench-loadgen warnings
  - Useful for debugging load generation issues
  - Shows rate limiting and connection pool messages

**Examples:**

```bash
# Default (compact, quiet) - recommended for CI
make bench

# Verbose mode for local debugging
BENCH_VERBOSE=1 LOADGEN_WARN=1 make bench

# Compact with warnings (troubleshoot loadgen issues)
BENCH_VERBOSE=0 LOADGEN_WARN=1 CASE=latency_short_1x make bench

# Full verbose for deep debugging
BENCH_VERBOSE=1 LOADGEN_WARN=1 PROXY=pavis CASE=reload_short_1x make bench
```

### Default Proxy Tags

The benchmark uses these Docker image tags by default:

| Proxy | Default Tag |
|-------|-------------|
| pavis | `local` (built from source) |
| envoy | `v1.32.2` |
| nginx | `1.26.2-alpine` |
| haproxy | `2.9.6-alpine` |

Override with environment variables:
```bash
ENVOY_TAG=v1.33.0 make bench PROXY=envoy
```

### Pavis Configuration

For `PROXY=pavis`, the benchmark automatically:
1. Generates `.pvs` binary config from `bench/config/pavis.yaml` using `pavctl gen`
2. Auto-builds `pavctl` if not found
3. Cleans up generated `.pvs` file after test completion

To customize pavis config, edit `bench/config/pavis.yaml`.

---

## 📈 Results & Troubleshooting

### Results Location

**Case-based benchmarks (`make bench`):**
- **Per-run output**: `bench/output/{proxy}/{case}/`
  - `wrk.txt` or `loadgen.txt` - Raw load generator output
  - `loadgen.txt.json` - JSON metrics (latency tests only)
  - `summary.json` - Parsed metrics
  - `meta.json` - Test metadata
  - `docker_stats.csv` - Container resource usage
- **Index**: `bench/output/{proxy}/index.csv`

**Full matrix benchmarks (`make benchmark`):**
- **Raw Output**: `bench/output/{proxy}/{proxy}.txt`
- **Aggregated CSV**: `bench/output/results.csv`
- **Summary Report**: `bench/output/summary.md`

### Troubleshooting

**"error: bench-loadgen not found"**
- Latency tests require `bench-loadgen` to be built. Run via `make bench` which automatically builds it, or build manually:
  ```bash
  cargo build -p pavis-benchkit --bin bench-loadgen --release
  ```

**"backend failed to become healthy after 30s"**
- Backend container failed to start. Check logs:
  ```bash
  docker logs bench-upstream
  ```
- Rebuild image if needed:
  ```bash
  make docker-build IMAGE=bench-upstream
  ```

**"pavis container exited with error"**
- Check pavis logs:
  ```bash
  docker logs bench-pavis
  ```
- Verify PVS config is valid:
  ```bash
  ./target/release/pavctl gen bench/config/pavis.yaml /tmp/test.pvs
  ```

**Slow on ARM Mac**
- Ensure bench-upstream is used (default).
- Check if containers are running native ARM images.

**Permission denied errors**
- Increase file descriptor limit:
  ```bash
  ulimit -n 10000
  ```

---

## Architecture

```
┌──────────────────┐      ┌─────────────────────┐      ┌──────────────────┐
│ wrk/bench-loadgen│ ───▶ │  Proxy (container)  │ ───▶ │ bench-upstream   │
│     (host)       │      │  CPU: 1-2           │      │ CPU: 0           │
│   4 threads      │      │  cgroup-limited     │      │ deterministic    │
└──────────────────┘      └─────────────────────┘      └──────────────────┘
                            ↓                              ↓
                       Pinned CPUs                   Pinned CPU
                       (isolation)                   (isolation)
```

### Components

**Load Generators:**
- **bench-loadgen** (open-loop): Rust-based load generator with fixed target RPS to avoid coordinated omission. Built from `crates/pavis-benchkit/src/bin/bench-loadgen.rs`.
- **wrk** (closed-loop): Maximum RPS throughput testing.

**Proxies:** Pavis (Rust/Pingora), Envoy (C++), Nginx (C), HAProxy (C).

**Backend:**
- **bench-upstream**: Deterministic backend from `crates/pavis-benchkit/src/bin/bench-upstream.rs`.
  - Compose service: `bench-upstream` (container `bench-upstream`).

**bench-upstream Endpoints:**
- `GET /healthz` -> `200 OK` with `ok`.
- `GET /fixed` -> fixed payload (`FIXED_BYTES`, default 64).
- `GET /status/{code}` -> specified status with fixed payload.
- `GET /sleep?ms=N` -> delayed fixed payload (capped at 10s).

---

## ⚡ Performance Tips (Linux)
Set the CPU governor to performance for stable results:
```bash
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

---

## File Structure

```
bench/
├── README.md              📖 This file (comprehensive guide)
├── QUICKSTART.md          ⚡ Quick reference guide
├── METHODOLOGY.md         🔬 Full methodology & Matrix
├── FAIRNESS.md            ⚖️ Config parity checklist
├── run.sh                 ▶️ Main benchmark runner with auto PVS management
├── docker-compose.yaml    🐳 Container definitions with default tags
├── config/                🛠️ Proxy configurations
│   ├── pavis.yaml         - Pavis config (auto-compiled to .pvs)
│   ├── envoy.yaml         - Envoy config
│   ├── nginx.conf         - Nginx config
│   └── haproxy.cfg        - HAProxy config
├── cases/                 🎯 Individual test case scripts
│   ├── throughput_short_1x.sh
│   ├── latency_short_1x.sh
│   ├── latency_extended_1x.sh
│   ├── concurrency_short_1x.sh
│   ├── churn_short_1x.sh
│   └── reload_short_1x.sh
├── scripts/               ✨ Legacy runner, CSV, and Summary scripts
└── output/                📁 Results & Reports
    └── {proxy}/
        ├── {case}/{timestamp}/  - Per-run detailed results
        └── index_{timestamp}.csv - Run index
```

---

## 🔧 Implementation Details

### Benchmark Runner (`bench/run.sh`)

The main runner provides:
- **Auto Build**: Automatically builds `bench-loadgen` if not present
- **Auto PVS Management**: Automatically generates and cleans up `.pvs` config for pavis
- **Case Orchestration**: Runs selected test cases sequentially
- **Dry-Run Mode**: Quick validation without actual benchmarks
- **Result Indexing**: Aggregates results into CSV index

### Test Case Scripts (`bench/cases/*.sh`)

Each test case is self-contained and includes:
- Container startup and health checks
- CPU pinning validation
- Warmup runs
- Docker stats collection
- Result parsing and JSON output
- Dry-run support for fast validation

### Docker Compose Setup

- **Isolation**: Separate CPU cores for proxy (1-2) and backend (0)
- **Resource Limits**: Configurable via `CPU_LIMIT` and `MEMORY_LIMIT`
- **Default Tags**: Pre-configured versions for reproducibility
- **Build Context**: Local builds for pavis and bench-upstream

---

## Limitations & Known Issues

1. **macOS Compatibility**:
   - CPU pinning (`cpuset`) and `/proc/cpuinfo` not available on macOS
   - CPU governor settings only work on Linux
   - Benchmark still runs but without CPU isolation guarantees

2. **Reload Benchmark**:
   - Config reload triggering mechanism pending implementation

3. **Single-Host**:
   - No multi-node distributed testing

4. **HTTP/1.1 Only**:
   - Current tests do not cover HTTP/2 or gRPC

See [METHODOLOGY.md](./METHODOLOGY.md#limitations--known-issues) for full details.

---

## 🆘 Support
- **Issues**: https://github.com/fabian4/pavis/issues

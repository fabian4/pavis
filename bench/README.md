# Pavis Benchmark

Performance comparison of Pavis against industry-standard proxies with focus on:
- **Open-loop latency testing** (wrk2) to avoid coordinated omission.
- **Backend bottleneck elimination** (bench-upstream only).
- **Statistical validation** (multi-run with median/IQR).
- **CPU isolation** (pinned cores for proxy, backend, load generator).
- **Configuration fairness** (documented semantic equivalence).

📖 **Detailed References**
- **[METHODOLOGY.md](./METHODOLOGY.md)**: Scientific foundations, metric definitions, and the full test matrix.
- **[FAIRNESS.md](./FAIRNESS.md)**: Detailed proxy configuration parity checklist.

---

## 🚀 Quick Start

### 1. Prerequisites
- `wrk` (or `wrk2` for latency tests)
- `docker` and `docker-compose`
- `bc` (for statistics)
- `ulimit -n 10000` (recommended)

### 2. Backend Selection
All benchmark runs use **bench-upstream** as the single canonical backend.
The runner automatically uses the correct backend configuration.

### 3. ARM Mac Users (M1/M2/M3)
Use bench-upstream to avoid Rosetta emulation overhead:
```bash
BENCHMARK_TARGET=pavis bash bench/scripts/run.sh
```

### 4. Execution Commands

**Test Single Proxy (Single run, ~5 mins):**
```bash
BENCHMARK_TARGET=pavis bash bench/scripts/run.sh
```

**Full Matrix (All proxies, ~45 mins):**
```bash
make benchmark
```

**Statistical Validation (N=5 iterations):**
```bash
BENCHMARK_RUNS=5 make benchmark
```

---

## 📈 Results & Troubleshooting

### Results Location
- **Raw Output**: `bench/output/{proxy}/{proxy}.txt`
- **Aggregated CSV**: `bench/output/results.csv`
- **Summary Report**: `bench/output/summary.md`

### Troubleshooting
- **"bench-backend is unhealthy"**: Ensure the backend container is running.
- **"wrk2 not found"**: Open-loop tests will fallback to standard `wrk` (closed-loop).
- **Slow on ARM Mac**: Ensure bench-upstream is used (default).

---

## Architecture

```
┌─────────────┐      ┌─────────────────────┐      ┌──────────────────┐
│  wrk/wrk2   │ ───▶ │  Proxy (container)  │ ───▶ │ bench-upstream   │
│  (host)     │      │  CPU: 1-2           │      │ CPU: 0           │
│  4 threads  │      │  cgroup-limited     │      │ deterministic    │
└─────────────┘      └─────────────────────┘      └──────────────────┘
                            ↓                              ↓
                       Pinned CPUs                   Pinned CPU
                       (isolation)                   (isolation)
```

### Components

**Load Generators:**
- **wrk2** (open-loop): Fixed target RPS to avoid coordinated omission.
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
├── README.md              📖 This file
├── METHODOLOGY.md         🔬 Full methodology & Matrix
├── FAIRNESS.md            ⚖️ Config parity checklist
├── bench.yaml             ⚙️ Matrix specification
├── docker-compose.yaml    🐳 Container definitions
├── config/                🛠️ Proxy configurations
├── scripts/               ✨ Runner, CSV, and Summary scripts
└── output/                📁 Results & Reports
```

---

## Limitations & Known Issues
1. **Reload Benchmark**: Triggering mechanism pending implementation.
2. **Single-Host**: No multi-node distributed testing.
3. **HTTP/1.1 Only**: Current tests do not cover HTTP/2 or gRPC.

See [METHODOLOGY.md](./METHODOLOGY.md#limitations--known-issues) for full details.

---

## 🆘 Support
- **Issues**: https://github.com/fabian4/pavis/issues

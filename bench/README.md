# Pavis Benchmark

Performance comparison of Pavis against industry-standard proxies with focus on:
- **Open-loop latency testing** (wrk2) to avoid coordinated omission.
- **Backend bottleneck elimination** (minimal backend option).
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

### 2. ARM Mac Users (M1/M2/M3)
**Recommended:** Use the minimal backend to avoid Rosetta emulation overhead:
```bash
BACKEND_TYPE=minimal BENCHMARK_TARGET=pavis bash bench/scripts/run.sh
```

### 3. Execution Commands

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
- **"bench-backend is unhealthy"**: Use `BACKEND_TYPE=minimal`.
- **"wrk2 not found"**: Open-loop tests will fallback to standard `wrk` (closed-loop).
- **Slow on ARM Mac**: Ensure `BACKEND_TYPE=minimal` is set.

---

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

### Components

**Load Generators:**
- **wrk2** (open-loop): Fixed target RPS to avoid coordinated omission.
- **wrk** (closed-loop): Maximum RPS throughput testing.

**Proxies:** Pavis (Rust/Pingora), Envoy (C++), Nginx (C), HAProxy (C).

**Backends:**
- **httpbin**: Functional realism (kennethreitz/httpbin).
- **minimal**: Dataplane isolation (lightweight Go server).

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
├── backend/               🆕 Minimal backend server
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
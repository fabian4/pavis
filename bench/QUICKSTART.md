# ✅ Pavis Benchmark v2.0 - Installation Complete

## Quick Summary

All methodological improvements have been successfully implemented:

✅ Open-loop latency testing (wrk2 support)
✅ Minimal backend for dataplane isolation
✅ Multi-run statistical validation
✅ CPU pinning for resource isolation
✅ Enhanced metrics (29 CSV columns)
✅ Configuration fairness documentation
✅ Comprehensive methodology docs
✅ ARM Mac compatibility

---

## ⚠️ IMPORTANT: ARM Mac Users

**Recommended:** Use the minimal backend on ARM Macs (M1/M2/M3):

```bash
BACKEND_TYPE=minimal BENCHMARK_TARGET=pavis bash bench/scripts/run.sh
```

**Why?** The httpbin image uses linux/amd64 and may be slow to start via Rosetta emulation. The minimal backend is native ARM64 and starts instantly.

---

## 🚀 Quick Start Commands

### 1. Test Single Proxy (Recommended First Step)

**With minimal backend (fastest, recommended for ARM Mac):**
```bash
cd /Users/fabian/Project/fabian/pavis
BACKEND_TYPE=minimal BENCHMARK_TARGET=pavis bash bench/scripts/run.sh
```

**With httpbin backend (more realistic):**
```bash
BENCHMARK_TARGET=pavis bash bench/scripts/run.sh
```

**Duration:** ~5-10 minutes (11 tests)

### 2. Full Benchmark Matrix (All 4 Proxies)

```bash
# With minimal backend (recommended)
BACKEND_TYPE=minimal make benchmark

# Or with httpbin (if you have time to wait)
make benchmark
```

**Duration:** ~30-45 minutes (44 tests total)

### 3. Multi-Run for Statistical Validation

```bash
BACKEND_TYPE=minimal BENCHMARK_RUNS=5 BENCHMARK_TARGET=pavis bash bench/scripts/run.sh
```

**Duration:** ~25-30 minutes (5 iterations of each test)

---

##  Results Location

After running benchmarks, check:

```bash
# Per-proxy raw output
ls bench/output/pavis/

# Aggregated CSV (29 columns)
head bench/output/results.csv

# Formatted summary report
cat bench/output/summary.md
```

---

## 📊 What's New in v2.0

### Core Improvements

1. **Open-Loop Latency Testing**
   - Uses wrk2 when available (falls back to wrk)
   - Fixed target RPS to avoid coordinated omission
   - Reports: target vs achieved RPS, P99.9 latency

2. **Backend Bottleneck Elimination**
   - New minimal Go backend (39-byte response)
   - Backend saturation detection (CPU > 80%)
   - Choice: httpbin (realistic) vs minimal (dataplane isolation)

3. **Statistical Validation**
   - Multi-run support (N=5 iterations)
   - Median and IQR aggregation
   - Low IQR = stable performance

4. **Resource Isolation**
   - CPU pinning: Backend on CPU 0, Proxy on CPUs 1-2
   - Prevents interference between components
   - Better result reproducibility

5. **Enhanced Metrics**
   - 29 CSV columns (was 26)
   - New: load_type, backend_type, target_rps, p999_ms, rps_median/iqr, p99_median/iqr, backend_cpu, backend_saturated

### Documentation

- **QUICKSTART.md** - This file
- **README.md** - Overview and architecture
- **METHODOLOGY.md** - Full scientific methodology (300+ lines)
- **FAIRNESS.md** - Proxy configuration parity checklist
- **UPGRADE-SUMMARY.md** - Implementation tracking

---

## 🔧 Troubleshooting

### Issue: "container bench-backend is unhealthy"

**Solution:** Use minimal backend
```bash
BACKEND_TYPE=minimal BENCHMARK_TARGET=pavis bash bench/scripts/run.sh
```

### Issue: "ulimit -n is 256"

**Solution:** Increase file descriptor limit
```bash
ulimit -n 10000
BACKEND_TYPE=minimal BENCHMARK_TARGET=pavis bash bench/scripts/run.sh
```

### Issue: "wrk2 not found"

**Impact:** Open-loop tests will fall back to wrk (closed-loop)

**Solution (optional):**
```bash
# macOS
brew tap jabley/homebrew-wrk2
brew install wrk2

# Linux
git clone https://github.com/giltene/wrk2.git
cd wrk2 && make && sudo cp wrk2 /usr/local/bin/
```

### Issue: Slow benchmark on ARM Mac

**Solution:** Make sure you're using minimal backend:
```bash
BACKEND_TYPE=minimal BENCHMARK_TARGET=pavis bash bench/scripts/run.sh
```

---

## 📈 Understanding Results

### Load Types

- **open-loop**: wrk2 with fixed target RPS → best for latency measurement
- **closed-loop**: wrk maximizing throughput → best for RPS measurement

### Backend Types

- **httpbin**: Python application (realistic but may saturate)
- **minimal**: Go server (eliminates backend bottleneck)

### Key Metrics

| Metric | Good Value | Meaning |
|--------|-----------|---------|
| `achieved_rps` | High | Requests per second |
| `p99_ms` | <10ms | 99th percentile latency |
| `p999_ms` | <50ms | 99.9th percentile latency |
| `errors` | 0 | Socket errors |
| `backend_saturated` | false | Backend not bottleneck |
| `rps_iqr` | Low | Stable performance across runs |

### Checking for Backend Saturation

```bash
# After benchmark completes
cut -d',' -f1,7,27,28 bench/output/results.csv | column -t -s','
# Shows: proxy, backend_type, backend_cpu_pct, backend_saturated
```

If `backend_saturated = true`, the results reflect backend limits, not proxy performance. Re-run with `BACKEND_TYPE=minimal`.

---

## 🎯 Next Steps

1. ✅ **Run quick test**
   ```bash
   BACKEND_TYPE=minimal BENCHMARK_TARGET=pavis bash bench/scripts/run.sh
   ```

2. ✅ **Review results**
   ```bash
   cat bench/output/pavis/pavis.txt
   ```

3. ✅ **Check CSV output**
   ```bash
   head -20 bench/output/results.csv | column -t -s','
   ```

4. ✅ **Read methodology**
   ```bash
   cat bench/METHODOLOGY.md
   ```

5. ✅ **Run full matrix** (when ready)
   ```bash
   BACKEND_TYPE=minimal make benchmark
   ```

---

## 📚 File Structure

```
bench/
├── QUICKSTART.md          ⭐ This file (start here)
├── README.md              📖 Overview
├── METHODOLOGY.md         🔬 Full methodology
├── FAIRNESS.md            ⚖️ Config parity
├── UPGRADE-SUMMARY.md     📊 Implementation details
├── backend/
│   ├── minimal-server.go  🆕 Minimal HTTP backend
│   └── Dockerfile         🆕 Multi-stage Go build
├── config/
│   ├── pavis.yaml
│   ├── envoy.yaml
│   ├── nginx.conf
│   └── haproxy.cfg
├── scripts/
│   ├── run.sh             ✨ Enhanced: wrk2, multi-run, backends
│   ├── csv.sh             ✨ Enhanced: 29 metrics, statistics
│   └── summary.sh
└── output/                📁 Results go here
    ├── pavis/pavis.txt
    ├── results.csv
    └── summary.md
```

---

## ⚡ Performance Tips

### Get Consistent Results

```bash
# Set CPU governor to performance (Linux only)
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Increase file descriptor limit
ulimit -n 10000

# Use minimal backend
export BACKEND_TYPE=minimal
```

### Speed Up Benchmarks

```bash
# Run CI matrix only (4 tests instead of 11)
# Edit run.sh and comment out run_extended_matrix call

# Or run single workload
# Edit run.sh and comment out unwanted run_benchmark calls
```

---

## 🎓 Learn More

### Methodology

- **Coordinated Omission**: Why open-loop testing matters
  - Video: https://www.youtube.com/watch?v=lJ8ydIuPFeU
  - Read: `bench/METHODOLOGY.md` Section 2

- **Statistical Validity**: Why multi-run with median/IQR
  - Read: `bench/METHODOLOGY.md` Section 6

- **Backend Isolation**: Why minimal backend matters
  - Read: `bench/METHODOLOGY.md` Section 3

### Configuration Fairness

All proxies configured with equivalent semantics:
- HTTP/1.1, keepalive enabled, 2 workers
- See: `bench/FAIRNESS.md` for full parity checklist

---

## 💡 Examples

### Compare Latency with Different Backends

```bash
# Test with httpbin
BACKEND_TYPE=httpbin BENCHMARK_TARGET=pavis bash bench/scripts/run.sh

# Test with minimal
BACKEND_TYPE=minimal BENCHMARK_TARGET=pavis bash bench/scripts/run.sh

# Compare P99 latency
grep "latency_baseline_short_1x" bench/output/results.csv | cut -d',' -f1,7,18
```

### Multi-Run Statistical Analysis

```bash
# Run with N=5 iterations
BACKEND_TYPE=minimal BENCHMARK_RUNS=5 BENCHMARK_TARGET=pavis bash bench/scripts/run.sh

# Check stability (low IQR = good)
grep "latency_baseline_extended_1x" bench/output/results.csv | cut -d',' -f1,12,13,20,21
# Shows: proxy, rps_median, rps_iqr, p99_median, p99_iqr
```

---

## 🆘 Support

- **Issues**: https://github.com/fabian4/pavis/issues
- **Documentation**: See `bench/*.md` files
- **Upgrade Guide**: `bench/UPGRADE-SUMMARY.md`

---

**Happy Benchmarking! 🚀**

---

## Changelog

### v2.0 (2026-01-09)
- Added: wrk2 open-loop latency testing
- Added: Minimal backend for dataplane isolation
- Added: Multi-run statistical validation
- Added: CPU pinning for resource isolation
- Added: Backend saturation detection
- Added: Comprehensive documentation (METHODOLOGY.md, FAIRNESS.md)
- Enhanced: 29 CSV metrics (was 26)
- Fixed: ARM Mac compatibility
- Fixed: ulimit check for "unlimited" value

### v1.0 (Initial)
- Basic benchmark matrix with wrk
- 4 proxies × 11 configurations = 44 runs
- httpbin backend only
- Single-run results

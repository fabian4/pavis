# Benchmark Quick Start Guide

This is a quick reference for the most common benchmark commands and workflows.

## 🏃 Fastest Path to Results

### 1️⃣ Build Images (one-time setup)

```bash
# Build required images
make docker-build IMAGE=bench-upstream
make docker-build IMAGE=pavis
```

### 2️⃣ Validate Setup (20 seconds)

```bash
# Quick validation without running benchmarks
DRY_RUN=1 make bench
```

### 3️⃣ Run Single Benchmark (1 minute)

```bash
# Run one test case
make bench CASE="throughput_short_1x"
```

### 4️⃣ View Results

```bash
# Find the latest results
ls -lt bench/output/pavis/throughput_short_1x/

# View summary
cat bench/output/pavis/throughput_short_1x/*/summary.json
```

---

## 📋 Common Commands

### Quick Validation

```bash
# Validate all test cases (no wrk/wrk2 needed)
DRY_RUN=1 make bench

# Validate specific cases
DRY_RUN=1 make bench CASE="throughput_short_1x latency_short_1x"

# Validate different proxy
DRY_RUN=1 make bench PROXY=nginx
```

### Running Benchmarks

```bash
# Single case
make bench CASE="throughput_short_1x"

# Multiple cases
make bench CASE="throughput_short_1x concurrency_short_1x"

# All default cases (6 tests, ~15-20 min)
make bench

# Skip latency tests (no wrk2 needed)
make bench CASE="throughput_short_1x concurrency_short_1x churn_short_1x"
```

### Different Proxies

```bash
# Test nginx
make bench PROXY=nginx CASE="throughput_short_1x"

# Test envoy
make bench PROXY=envoy CASE="throughput_short_1x"

# Test haproxy
make bench PROXY=haproxy CASE="throughput_short_1x"
```

### Custom Tags

```bash
# Use different envoy version
ENVOY_TAG=v1.33.0 make bench PROXY=envoy

# Use different nginx version
NGINX_TAG=1.27-alpine make bench PROXY=nginx
```

---

## 🎯 Available Test Cases

| Case | Tool | Duration | Tests |
|------|------|----------|-------|
| `throughput_short_1x` | wrk | 30s | Max RPS |
| `latency_short_1x` | wrk2* | 30s | Latency P50/P90/P99 |
| `latency_extended_1x` | wrk2* | 120s | Tail latency stability |
| `concurrency_short_1x` | wrk | 30s | High connection count |
| `churn_short_1x` | wrk | 30s | Connection churn |
| `reload_short_1x` | wrk2* | 60s | Config reload impact |

\* Requires `wrk2` - use `DRY_RUN=1` to validate without it

---

## 🔍 Viewing Results

### Latest Results

```bash
# Find latest run for a case
ls -lt bench/output/pavis/throughput_short_1x/ | head -2

# View summary JSON
cat bench/output/pavis/throughput_short_1x/*/summary.json | jq .

# View raw wrk output
cat bench/output/pavis/throughput_short_1x/*/wrk.txt
```

### Index File

```bash
# View latest index (all runs)
ls -t bench/output/pavis/index_*.csv | head -1 | xargs cat
```

### Docker Stats

```bash
# View resource usage during test
cat bench/output/pavis/throughput_short_1x/*/docker_stats.csv
```

---

## 🛠️ Configuration

### Pavis Config

Edit `bench/config/pavis.yaml` to customize pavis settings. The benchmark will automatically:
1. Generate `.pvs` binary from YAML
2. Use it for the test
3. Clean it up afterward

### Proxy Versions (Default Tags)

| Proxy | Default Tag | Override |
|-------|-------------|----------|
| pavis | `local` | N/A (built from source) |
| envoy | `v1.32.2` | `ENVOY_TAG=...` |
| nginx | `1.26.2-alpine` | `NGINX_TAG=...` |
| haproxy | `2.9.6-alpine` | `HAPROXY_TAG=...` |

---

## ⚡ Performance Tips

### macOS

The benchmark works on macOS but has limitations:
- No CPU pinning (cpuset not available)
- No CPU governor controls
- Still provides valid comparative results

### Linux

For best results on Linux:

```bash
# Set CPU governor to performance
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Increase file descriptor limit
ulimit -n 10000

# Disable CPU frequency scaling
sudo cpupower frequency-set --governor performance
```

---

## 🐛 Troubleshooting

### "error: missing required command 'wrk2'"

Skip latency tests or use dry-run:
```bash
# Skip latency tests
make bench CASE="throughput_short_1x concurrency_short_1x churn_short_1x"

# Or use dry-run
DRY_RUN=1 make bench
```

### "backend failed to become healthy"

Check backend logs and rebuild:
```bash
docker logs bench-upstream
make docker-build IMAGE=bench-upstream
```

### "pavis container exited"

Check pavis logs:
```bash
docker logs bench-pavis

# Verify PVS config
./target/release/pavctl gen bench/config/pavis.yaml /tmp/test.pvs
```

### No warnings about environment variables

All proxy tags now have defaults - no warnings expected!

---

## 📚 More Information

- Full documentation: [README.md](./README.md)
- Methodology: [METHODOLOGY.md](./METHODOLOGY.md)
- Configuration fairness: [FAIRNESS.md](./FAIRNESS.md)

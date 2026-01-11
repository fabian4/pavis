# CI Benchmark Guide

## Overview

The CI benchmark suite runs **5 short test cases** across **4 proxies** to validate performance characteristics in ~12-16 minutes total.

**Key Features:**
- ✅ Fast CI execution (12-16 minutes vs 58+ minutes)
- ✅ Tests all 4 proxies: pavis, envoy, nginx, haproxy
- ✅ Keeps raw outputs only (no intermediate JSON files)
- ✅ Single summary.csv for all results
- ✅ GitHub Actions artifacts saved for 30 days

## CI Configuration

### Proxies Tested
1. **pavis** - The main proxy implementation
2. **envoy** - Envoy proxy for comparison
3. **nginx** - Nginx for comparison
4. **haproxy** - HAProxy for comparison

### Test Cases (Short Variants - CI Only)

| Case                    | Duration | Runs | Target RPS | Connections | Purpose                           |
|-------------------------|----------|------|------------|-------------|-----------------------------------|
| `throughput_short_1x`   | 30s      | 1    | unlimited  | 100         | Max throughput (closed-loop)      |
| `latency_short_1x`      | 30s      | 1    | 10,000     | 500         | Latency under load (open-loop)    |
| `concurrency_short_1x`  | 30s      | 1    | unlimited  | 5,000       | High concurrency handling         |
| `churn_short_1x`        | 30s      | 1    | unlimited  | 100         | Connection churn resilience       |
| `reload_short_1x`       | 30s      | 5    | 5,000      | 500         | Config reload stability (5 runs)  |

**Total CI time:** ~12-16 minutes (5 cases × 4 proxies × ~30s + overhead)

### Excluded from CI

- **`latency_extended_1x`** - 25 minutes (300s × 5 runs) - reserved for local full benchmarks

## Output Structure

### Raw Outputs (Kept)

Each test produces **only raw outputs** - no intermediate summary.json files:

```
bench/output/
├── pavis/
│   ├── throughput_short_1x/
│   │   ├── meta.json           # Benchmark metadata
│   │   ├── wrk.txt             # Raw wrk output
│   │   ├── warmup.txt          # Warmup run
│   │   └── docker_stats.csv    # Resource usage
│   ├── latency_short_1x/
│   │   ├── meta.json
│   │   ├── loadgen.txt.json    # Raw bench-loadgen output
│   │   ├── warmup.txt.json
│   │   └── docker_stats.csv
│   ├── reload_short_1x/
│   │   ├── meta.json
│   │   ├── aggregate.json      # Median/IQR statistics
│   │   ├── rps_values.txt      # Raw RPS values from all runs
│   │   ├── p99_values.txt      # Raw P99 values from all runs
│   │   ├── run_1/
│   │   │   ├── loadgen.txt.json
│   │   │   ├── warmup.txt.json
│   │   │   └── docker_stats.csv
│   │   ├── run_2/
│   │   ├── run_3/
│   │   ├── run_4/
│   │   └── run_5/
│   └── [other cases...]
├── envoy/
│   └── [same structure]
├── nginx/
│   └── [same structure]
├── haproxy/
│   └── [same structure]
└── summary.csv              # ⭐ Aggregated results from all tests
```

### Summary CSV Format

The `summary.csv` contains one row per test case per proxy:

```csv
proxy,case,type,runs,achieved_rps,p50_ms,p90_ms,p99_ms,errors,dropped,rps_iqr,p99_iqr
pavis,throughput_short_1x,wrk,1,26310.45,0.450,0.890,1.230,0,,,
pavis,latency_short_1x,loadgen-single,1,9992.466666666667,0.652,0.943,1.557,0,0,,
pavis,reload_short_1x,loadgen-multi,5,4998.234,,,1.423,0,,12.456,0.045
envoy,throughput_short_1x,wrk,1,24567.89,0.520,0.950,1.340,0,,,
...
```

**Columns:**
- `proxy`: pavis, envoy, nginx, haproxy
- `case`: Test case name
- `type`: wrk, loadgen-single, loadgen-multi
- `runs`: Number of runs (1 for single, 5 for multi)
- `achieved_rps`: Actual requests/sec (median for multi-run)
- `p50_ms`, `p90_ms`, `p99_ms`: Latency percentiles in milliseconds
- `errors`: Failed requests
- `dropped`: Dropped requests (open-loop saturation)
- `rps_iqr`, `p99_iqr`: Interquartile range (multi-run only)

## Running Benchmarks

### Local CI Benchmark (Fast)

```bash
# Run all 4 proxies with short cases
for proxy in pavis envoy nginx haproxy; do
  PROXY=$proxy CASE="throughput_short_1x latency_short_1x concurrency_short_1x churn_short_1x reload_short_1x" make bench
done

# Generate summary
bash bench/summarize.sh

# View results
cat bench/output/summary.csv
```

### Single Proxy

```bash
# Run pavis with short cases
PROXY=pavis CASE="throughput_short_1x latency_short_1x concurrency_short_1x churn_short_1x reload_short_1x" make bench

# Generate summary
bash bench/summarize.sh
```

### Full Benchmark Suite (Local Only)

```bash
# Includes latency_extended_1x (25 minutes)
PROXY=pavis make bench

# Generate summary
bash bench/summarize.sh
```

### Dry-Run Validation

```bash
# Validate setup without running benchmarks
DRY_RUN=1 PROXY=pavis make bench
```

## Analyzing Results

### View Summary Table

```bash
# Pretty-printed table
column -t -s, bench/output/summary.csv
```

### Compare Proxies

```bash
# Filter by case
grep "latency_short_1x" bench/output/summary.csv

# Compare RPS across proxies
awk -F, 'NR>1 {print $1,$2,$5}' bench/output/summary.csv | column -t
```

### Import to Excel/Sheets

The `summary.csv` can be directly imported into Excel, Google Sheets, or any data analysis tool.

## Key Metrics

### Interpreting Results

- **achieved_rps**: Higher is better (throughput capacity)
- **p99_ms**: Lower is better (tail latency)
- **errors**: Should be 0 (reliability)
- **dropped**: >0 indicates saturation (target RPS not sustainable)
- **rps_iqr**, **p99_iqr**: Lower is better (consistency across runs)

### Warning Signs

- **dropped > 0**: System saturated, reduce target RPS
- **errors > 0**: Connection failures or timeouts
- **p99_ms > 10ms**: May indicate backend saturation or proxy inefficiency

## CI Workflow

1. **Builds** pavis and bench-upstream Docker images
2. **Runs** 5 short cases for each of 4 proxies (20 benchmarks total)
3. **Generates** summary.csv with all results
4. **Displays** summary in CI logs
5. **Uploads** all outputs as GitHub Actions artifact (30-day retention)

## Next Steps

After running benchmarks:

1. **Download** GitHub Actions artifact containing `bench/output/`
2. **Analyze** `summary.csv` to compare proxy performance
3. **Investigate** raw outputs (`wrk.txt`, `loadgen.txt.json`) for details
4. **Visualize** results (create charts from CSV)
5. **Optimize** pavis based on comparison with competitors


# Benchmark Analysis Report

## 1. Overview

**Generated:** `2025-12-26T05:21:13Z` ｜ **Runs:** `44` · **Baseline:** `envoy`

**Version:** `bench-20251226` (`37c678ee82cfa79c844bc3ba00e4d4af28b251f6`)

> Proxies: `envoy(v1.32.2)` · `haproxy(2.9.6-alpine)` · `nginx(1.26.2-alpine)` · `pavis(bench-20251226)`  
> Workloads: `churn` · `concurrency` · `latency` · `throughput`  
> Profiles: `baseline` · `cpu-limited` · `memory-limited`

---

## 2. Baseline Consolidated Table

Filters: resource_profile = baseline, duration_s = 30

| Proxy | Throughput RPS | Latency RPS | Concurrency RPS | Churn RPS | Avg CPU (%) | Avg Memory (MiB) |
|-------|----------------|-------------|-----------------|-----------|-------------|------------------|
| envoy | 1173.05 | 1186.28 | 5916.14 | 897.57 | 47.5 | 102 |
| haproxy | 1312.45 | 1314.77 | 2889.99 | 646.70 | 23.5 | 44 |
| nginx | 1344.16 | 1324.71 | 1317.89 | 1029.86 | 21.5 | 35 |
| pavis | 1034.87 | 1033.75 | 964.85 | 731.84 | 33.2 | 215 |

---

## 3. Workload Performance Tables

### Throughput (baseline, 30s, 100 connections)

| Proxy | RPS (Δ) | P99 Latency (ms) | Errors | Avg CPU (RPS/CPU) | Avg Mem (RPS/MiB) |
|-------|------------------|------------------|--------|-------------------|-------------------|
| envoy | 1173.05 | 182.77 | 0 | 36.77 (31.90) | 24.65 (47.59) |
| haproxy | 1312.45 (+11.9%) | 152.80 | 0 | 20.79 (63.13) | 12.17 (107.84) |
| nginx | 1344.16 (+14.6%) | 182.14 | 0 | 20.86 (64.44) | 12.48 (107.71) |
| pavis | 1034.87 (-11.8%) | 186.68 | 0 | 32.01 (32.33) | 11.24 (92.07) |

### Latency (baseline, 30s, 500 connections)

| Proxy | RPS (Δ) | P99 Latency (ms) | Errors | Avg CPU (RPS/CPU) | Avg Mem (RPS/MiB) |
|-------|------------------|------------------|--------|-------------------|-------------------|
| envoy | 1186.28 | 823.67 | 0 | 36.90 (32.15) | 41.17 (28.81) |
| haproxy | 1314.77 (+10.8%) | 875.67 | 60 | 20.48 (64.20) | 18.06 (72.80) |
| nginx | 1324.71 (+11.7%) | 843.11 | 31 | 21.81 (60.74) | 18.83 (70.35) |
| pavis | 1033.75 (-12.9%) | 606.21 | 52 | 32.04 (32.26) | 41.18 (25.10) |

### Concurrency (baseline, 30s, 5000 connections)

| Proxy | RPS (Δ) | P99 Latency (ms) | Errors | Avg CPU (RPS/CPU) | Avg Mem (RPS/MiB) |
|-------|------------------|------------------|--------|-------------------|-------------------|
| envoy | 5916.14 | 1900 | 7706 | 76.09 (77.75) | 174.01 (34.00) |
| haproxy | 2889.99 (-51.2%) | 852.05 | 4176 | 29.44 (98.17) | 84.44 (34.23) |
| nginx | 1317.89 (-77.7%) | 1000 | 593 | 21.39 (61.61) | 97.31 (13.54) |
| pavis | 964.85 (-83.7%) | 1570 | 2042 | 33.94 (28.43) | 399.38 (2.42) |

### Concurrency (2x intensity, 30s, 10000 connections)

| Proxy | RPS (Δ) | P99 Latency (ms) | Errors | Avg CPU (RPS/CPU) | Avg Mem (RPS/MiB) |
|-------|------------------|------------------|--------|-------------------|-------------------|
| envoy | 5829.63 | 1910 | 21493 | 75.75 (76.96) | 298.76 (19.51) |
| haproxy | 4426.42 (-24.1%) | 1440 | 23086 | 37.28 (118.73) | 178.05 (24.86) |
| nginx | 561.26 (-90.4%) | 1750 | 2203 | 24.47 (22.94) | 179.42 (3.13) |
| pavis | 885.85 (-84.8%) | 1840 | 6741 | 39.11 (22.65) | 452.63 (1.96) |

### Churn (baseline, 30s, 100 connections)

| Proxy | RPS (Δ) | P99 Latency (ms) | Errors | Avg CPU (RPS/CPU) | Avg Mem (RPS/MiB) |
|-------|------------------|------------------|--------|-------------------|-------------------|
| envoy | 897.57 | 194.09 | 0 | 40.31 (22.27) | 169.52 (5.29) |
| haproxy | 646.70 (-27.9%) | 1070 | 7 | 23.36 (27.68) | 61.36 (10.54) |
| nginx | 1029.86 (+14.7%) | 188.11 | 0 | 22.13 (46.54) | 12.76 (80.71) |
| pavis | 731.84 (-18.5%) | 562.67 | 0 | 34.75 (21.06) | 407.62 (1.80) |

---

## 4. Stability (30s vs 300s)

| Proxy | Workload | RPS (30s) | RPS (300s) | Delta (%) |
|-------|----------|-----------|------------|-----------|
| envoy | throughput | 1173.05 | 1208.76 | +3.0% |
| envoy | latency | 1186.28 | 1194.28 | +0.7% |
| haproxy | throughput | 1312.45 | 1355.40 | +3.3% |
| haproxy | latency | 1314.77 | 1331.91 | +1.3% |
| nginx | throughput | 1344.16 | 1348.97 | +0.4% |
| nginx | latency | 1324.71 | 1307.16 | -1.3% |
| pavis | throughput | 1034.87 | 1074.64 | +3.8% |
| pavis | latency | 1033.75 | 1066.04 | +3.1% |

---

## 5. Resource Efficiency

Filter: throughput workload, resource_profile=baseline, duration=30s

| Proxy | Avg CPU (%) | Peak CPU (%) | Avg Mem (MiB) | Peak Mem (MiB) | RPS/CPU | RPS/MiB |
|-------|-------------|--------------|---------------|----------------|---------|---------|
| envoy | 36.77 | 38.56 | 24.65 | 24.88 | 31.90 | 47.59 |
| haproxy | 20.79 | 21.55 | 12.17 | 13.23 | 63.13 | 107.84 |
| nginx | 20.86 | 21.96 | 12.48 | 12.75 | 64.44 | 107.71 |
| pavis | 32.01 | 33.51 | 11.24 | 11.75 | 32.33 | 92.07 |

---

## 6. Error Overview

| Workload | Connections | envoy | haproxy | nginx | pavis |
|----------|-------------|-------|-------|-------|-------|
| throughput | 100 | 0 | 0 | 0 | 0 |
| latency | 500 | 0 | 60 | 31 | 52 |
| latency (2x) | 1000 | 515 | 574 | 530 | 487 |
| concurrency | 5000 | 7706 | 4176 | 593 | 2042 |
| concurrency (2x) | 10000 | 21493 | 23086 | 2203 | 6741 |
| churn | 100 | 0 | 7 | 0 | 0 |

---

## 7. Key Findings

**Throughput:** nginx > haproxy > envoy > pavis

**Latency:** nginx > haproxy > envoy > pavis

**Concurrency:** envoy > haproxy > nginx > pavis

**Churn:** nginx > envoy > pavis > haproxy

**Lowest CPU:** haproxy (20.79%)

**Lowest Memory:** pavis (11.24 MiB)

**Errors observed:** latency, concurrency, churn

---

All results are derived directly from results.csv.

# Benchmark Analysis Report

## 1. Overview

**Generated:** `2026-01-07T15:50:39Z` ｜ **Runs:** `22` · **Baseline:** `envoy`

**Version:** `no-tag` (`32f8b372a31bc2eb2f270a46ab28dc1a5747b554`)

> Proxies: `envoy(v1.32.2)` · `pavis(no-tag)`  
> Workloads: `churn` · `concurrency` · `latency` · `throughput`  
> Profiles: `baseline` · `cpu-limited` · `memory-limited`

---

## 2. Baseline Consolidated Table

Filters: resource_profile = baseline, duration_s = 30

| Proxy | Throughput RPS | Latency RPS | Concurrency RPS | Churn RPS | Avg CPU (%) | Avg Memory (MiB) |
|-------|----------------|-------------|-----------------|-----------|-------------|------------------|
| envoy | 1479.90 | 1472.53 | 14012.61 | 1229.15 | 70.3 | 104 |
| pavis | 1512.31 | 1492.19 | 1731.90 | 1124.75 | 59.2 | 212 |

---

## 3. Workload Performance Tables

### Throughput (baseline, 30s, 100 connections)

| Proxy | RPS (Δ) | P99 Latency (ms) | Errors | Avg CPU (RPS/CPU) | Avg Mem (RPS/MiB) |
|-------|------------------|------------------|--------|-------------------|-------------------|
| envoy | 1479.90 | 82.00 | 0 | 35.86 (41.27) | 25.65 (57.70) |
| pavis | 1512.31 (+2.2%) | 79.72 | 0 | 33.04 (45.77) | 17.77 (85.10) |

### Latency (baseline, 30s, 500 connections)

| Proxy | RPS (Δ) | P99 Latency (ms) | Errors | Avg CPU (RPS/CPU) | Avg Mem (RPS/MiB) |
|-------|------------------|------------------|--------|-------------------|-------------------|
| envoy | 1472.53 | 397.18 | 0 | 36.05 (40.85) | 42.70 (34.49) |
| pavis | 1492.19 (+1.3%) | 845.37 | 0 | 33.43 (44.64) | 46.48 (32.10) |

### Concurrency (baseline, 30s, 5000 connections)

| Proxy | RPS (Δ) | P99 Latency (ms) | Errors | Avg CPU (RPS/CPU) | Avg Mem (RPS/MiB) |
|-------|------------------|------------------|--------|-------------------|-------------------|
| envoy | 14012.61 | 1220 | 2996 | 158.15 (88.60) | 180.06 (77.82) |
| pavis | 1731.90 (-87.6%) | 882.14 | 9845 | 46.54 (37.21) | 377.78 (4.58) |

### Concurrency (2x intensity, 30s, 10000 connections)

| Proxy | RPS (Δ) | P99 Latency (ms) | Errors | Avg CPU (RPS/CPU) | Avg Mem (RPS/MiB) |
|-------|------------------|------------------|--------|-------------------|-------------------|
| envoy | 13584.60 | 1750 | 4833 | 159.64 (85.10) | 306.24 (44.36) |
| pavis | 1473.38 (-89.2%) | 1940 | 21319 | 70.64 (20.86) | 370.74 (3.97) |

### Churn (baseline, 30s, 100 connections)

| Proxy | RPS (Δ) | P99 Latency (ms) | Errors | Avg CPU (RPS/CPU) | Avg Mem (RPS/MiB) |
|-------|------------------|------------------|--------|-------------------|-------------------|
| envoy | 1229.15 | 99.33 | 0 | 51.23 (23.99) | 168.77 (7.28) |
| pavis | 1124.75 (-8.5%) | 126.80 | 0 | 123.98 (9.07) | 404.12 (2.78) |

---

## 4. Stability (30s vs 300s)

| Proxy | Workload | RPS (30s) | RPS (300s) | Delta (%) |
|-------|----------|-----------|------------|-----------|
| envoy | throughput | 1479.90 | 1502.86 | +1.6% |
| envoy | latency | 1472.53 | 1492.45 | +1.4% |
| pavis | throughput | 1512.31 | 1495.26 | -1.1% |
| pavis | latency | 1492.19 | 1484.52 | -0.5% |

---

## 5. Resource Efficiency

Filter: throughput workload, resource_profile=baseline, duration=30s

| Proxy | Avg CPU (%) | Peak CPU (%) | Avg Mem (MiB) | Peak Mem (MiB) | RPS/CPU | RPS/MiB |
|-------|-------------|--------------|---------------|----------------|---------|---------|
| envoy | 35.86 | 37.47 | 25.65 | 26.56 | 41.27 | 57.70 |
| pavis | 33.04 | 33.96 | 17.77 | 19.34 | 45.77 | 85.10 |

---

## 6. Error Overview

| Workload | Connections | envoy | pavis |
|----------|-------------|-------|-------|
| throughput | 100 | 0 | 0 |
| latency | 500 | 0 | 0 |
| latency (2x) | 1000 | 401 | 508 |
| concurrency | 5000 | 2996 | 9845 |
| concurrency (2x) | 10000 | 4833 | 21319 |
| churn | 100 | 0 | 0 |

---

## 7. Key Findings

**Throughput:** pavis > envoy

**Latency:** pavis > envoy

**Concurrency:** envoy > pavis

**Churn:** envoy > pavis

**Lowest CPU:** pavis (33.04%)

**Lowest Memory:** pavis (17.77 MiB)

**Errors observed:** concurrency

---

All results are derived directly from results.csv.

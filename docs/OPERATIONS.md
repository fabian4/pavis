# Operations

## 1. Benchmarks

Performance comparison of Pavis against industry-standard proxies (Envoy, Nginx, HAProxy).

**CI:** [GitHub Actions](https://github.com/fabian4/pavis/actions/workflows/bench.yaml)

### Reports

| Date | Version | Highlights |
|------|---------|------------|
| [2025-12-26](./report/bench-20251226/report.md) | `bench-20251226` | 🏆 Notable memory efficiency · 📉 12–15% throughput gap · 🚧 Concurrency bottleneck |

### Methodology

- **Proxies:** Envoy, HAProxy, Nginx, Pavis
- **Workloads:** throughput, latency, concurrency, churn
- **Profiles:** baseline, cpu-limited, memory-limited

Detailed reports are archived in `bench/report/`.

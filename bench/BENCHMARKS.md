# Benchmark Reports

Performance comparison of Pavis against industry-standard proxies (Envoy, Nginx, HAProxy).

**CI:** [GitHub Actions](https://github.com/fabian4/pavis/actions/workflows/bench.yaml)

## Reports

| Date | Version | Highlights | CI Run |
|------|---------|------------|--------|
| [2025-12-26](./report/bench-20251226/report.md) | `bench-20251226` | 🏆 Notable memory efficiency · 📉 12–15% throughput gap · 🚧 Concurrency bottleneck | [#20516504677](https://github.com/fabian4/pavis/actions/runs/20516504677) |

## Summary

- **Proxies:** Envoy, HAProxy, Nginx, Pavis
- **Workloads:** throughput, latency, concurrency, churn
- **Profiles:** baseline, cpu-limited, memory-limited

See [README.md](./README.md) for detailed methodology.

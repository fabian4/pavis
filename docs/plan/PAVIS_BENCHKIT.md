# Execution Plan: Pavis Benchkit Crate (Revised)

This document outlines the architectural plan to introduce `crates/pavis-benchkit`, a specialized utility crate providing a deterministic minimal backend for data-plane performance isolation.

## Goals
- Provide a high-performance, low-latency HTTP backend (`bench-upstream`) to isolate proxy overhead from application overhead.
- Ensure deterministic response semantics and payloads to facilitate baseline comparisons.
- Best-effort minimization of latency variance through architectural simplicity.
- Consolidate benchmarking utilities into a formal Rust crate without contaminating E2E test infrastructure.

## Non-Goals
- Support for complex protocol features (HTTP/2, gRPC, WebSockets).
- Inbound TLS termination (benchmarks target proxy TLS performance).
- Absolute zero jitter (unavoidable due to OS scheduler and hardware noise).

## Proposed Crate Layout
- `crates/pavis-benchkit/Cargo.toml`: Minimal dependencies strictly limited to `tokio`, `hyper`, `http`, and `bytes`.
- `crates/pavis-benchkit/src/lib.rs`: Shared types for metrics and configuration.
- `crates/pavis-benchkit/src/metrics.rs`: Feature-gated, atomic counters. Metrics and procfs support are `OFF` by default.
- `crates/pavis-benchkit/src/bin/bench-upstream.rs`: Primary backend binary. `axum`, `tower`, `tracing`, and middleware stacks are explicitly excluded to minimize variance.
- `crates/pavis-benchkit/Dockerfile`: Multi-stage build for the backend image.

## Binary Specs: bench-upstream
The `bench-upstream` binary acts as the terminal node in the benchmark topology.

- **Runtime & Framework**: `tokio` (multi-threaded) with pure `hyper` handlers. Pure `hyper` is used to eliminate hidden framework allocations, automatic header injection, and tail-latency variance introduced by higher-level stacks.
- **Networking**: Full support for HTTP/1.1 keepalive.
- **Logging**: Default to `OFF`. Only enabled via `RUST_LOG` for environment debugging.
- **Lifecycle**: Graceful shutdown on `SIGTERM` to preserve state for final metric scraping.

## HTTP API Contract
- `GET /healthz`: Returns `200 OK` with body `ok`. `Content-Type: text/plain`.
- `GET /fixed`: Returns `200 OK`. `Content-Type: application/octet-stream`. Body length is controlled by `FIXED_BYTES` (default 64).
- `GET /status/{code}`: Responds with specified status; static fixed payload. `Content-Type: application/octet-stream`. Accepts codes 100–599; invalid values return 400.
- `GET /sleep?ms=N`: Asynchronously sleeps for `N` ms (capped at 10s) then returns fixed payload. `Content-Type: application/octet-stream`. Invalid or missing `ms` returns 400.

### Payload Construction
The fixed payload is constructed exactly once at startup and reused for all subsequent requests via `Arc<[u8]>` to eliminate per-request heap allocation.

### HTTP Semantics (Required)
- **Content-Length**: Must always be present.
- **Transfer-Encoding**: `chunked` encoding is strictly forbidden.
- **Keepalive**: Use HTTP/1.1 keepalive by default.
- **Connection Closure**: Respect `Connection: close` headers and ensure closed connections are not reused.

## Determinism Rules
To ensure best-effort minimization of noise, the handler adheres to:
- **Static Headers**: No `Date`, `Server`, or `X-Request-ID` headers are injected. `hyper` must be configured to prevent automatic header injection.
- **Fixed Formatting**: No compression is permitted.
- **Uniformity**: All status codes (including errors) must return the same uniform response body to ensure constant-time serialization. No dynamic error pages or formatting are allowed.
- **Pre-allocation**: All response buffers are pre-allocated at startup.

## Observability
- **Primary Source**: `docker stats` remains the source of truth for `backend_cpu_pct` and `backend_saturated`.
- **Secondary Source**: `/metrics` (Prometheus text format) used for sanity checks (request counts).
- **Metric Discipline**: Scraping occurs only before and after benchmark windows to avoid interference.
- **Resource Stats**: RSS and file descriptor metrics are feature-gated and `OFF` by default.

## Docker & Distribution
- **Strategy**: Default/official runtime image is glibc-based for consistency with production artifacts.
- **Base Image**: Official benchmark image uses `gcr.io/distroless/cc-debian12`. `debian-slim` and `alpine` are offered only as `:debug` variants for local troubleshooting.
- **Healthchecks**: No `curl` inside the image. Readiness is determined by the benchmark runner via external TCP/HTTP probes.
- **Reproducibility**: Benchmark reports MUST record the image **digest** (SHA256) for both the backend and the proxy under test.

## Bench Integration
Introducing a deterministic minimal backend under project control allows for precise dataplane isolation.
- **Integration**: `bench/docker-compose.yaml` service `backend-minimal` using the bench-upstream image.
- **Resource Isolation**: Pinned to CPU 0 via `cpuset_cpus`. For NUMA-aware hosts, `cpuset_mems` is recommended.
- **Saturation Semantics**: `backend_saturated` is true if the average CPU utilization exceeds 80% over the measurement window. Sampling interval is 1s. Mean-over-window is the primary signal; P95 is recorded for diagnostics.

## CI Integration & Reproducibility
- **Docker Builds**: Explicit platform specification required (`--platform linux/amd64,linux/arm64`).
- **Smoke Test**: CI step verifies `/healthz`, `/fixed` length, `/status/503`, `/sleep?ms=10`, and HTTP/1.1 keepalive stability.
- **Metadata Collection**: The benchmark runner performs metadata collection (kernel, CPU model, governor, image digests) at the start of each run. 
- **Metadata Persistence**: Collected metadata is persisted alongside results in `meta.json` and optionally included in CSV headers.

## Risks & Mitigations
- **Framework Overhead**: Mitigated by using pure `hyper` handlers and strictly disabling automatic headers.
- **System Noise**: Mitigated by pinning to distinct cores and recommending the `performance` governor.
- **Handler Starvation**:
    - Metrics scraping is restricted to periods outside the load window.
    - The `healthz` handler is constant-time and allocation-free.
    - The Tokio worker thread count is fixed and documented to prevent scheduler thrashing.
    - A feature-gated separate admin listener is available for diagnostics but remains disabled during official runs.

## Milestones
- **M0**: Crate skeleton + `hyper` binary + local smoke test.
- **M1**: Dockerfile (distroless) + image digest recording in runner.
- **M2**: Integration into `bench/docker-compose.yaml` with `cpuset` and average-CPU saturation logic.
- **M3**: Baseline stability comparison and final documentation.

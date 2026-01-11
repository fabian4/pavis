# Pavis Benchkit & bench-upstream

**Pavis Benchkit** is a specialized utility crate providing `bench-upstream`, a high-performance, deterministic HTTP backend designed for data-plane performance isolation in proxy benchmarks.

It is designed to eliminate "backend noise" (application runtime overhead, GC pauses, dynamic allocations) when measuring proxy latency and throughput.

## 🚀 bench-upstream

`bench-upstream` is the canonical backend for the Pavis benchmark suite. It acts as a minimal, compliant HTTP/1.1 server that returns pre-allocated payloads with constant-time serialization.

### Key Features

- **Deterministic Latency**: Pure `hyper` implementation (no web framework overhead) with zero per-request allocations for fixed payloads.
- **Stable Semantics**: No dynamic header injection (`Date`, `Server` are omitted).
- **Controlled Load**: Explicit support for simulating upstream latency (`/sleep`) and status codes (`/status/:code`).
- **Resource Isolation**: Configurable worker thread count to prevent scheduler thrashing.
- **Safety**: Graceful handling of connection close and keepalive.

### 🔌 API Endpoints

| Method | Path | Description | Response Type |
|--------|------|-------------|---------------|
| `GET` | `/healthz` | Health probe. Returns `200 OK` with body `ok`. | `text/plain` |
| `GET` | `/fixed` | Returns fixed-size payload (default 64 bytes). | `application/octet-stream` |
| `GET` | `/status/{code}` | Returns specified HTTP status (100-599) with fixed payload. | `application/octet-stream` |
| `GET` | `/sleep?ms=N` | Sleeps for `N` ms (capped), then returns fixed payload. | `application/octet-stream` |
| `GET` | `/metrics` | Prometheus metrics (request counts), if enabled. | `text/plain` |

### ⚙️ Configuration

Configuration is handled via environment variables.

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8000` | Listening port. |
| `FIXED_BYTES` | `64` | Size of the pre-allocated response payload in bytes. |
| `SLEEP_CAP_MS` | `10000` | Maximum allowed sleep duration for `/sleep` endpoint. |
| `WORKER_THREADS`| `2` | Number of Tokio worker threads. |
| `RUST_LOG` | `off` | Set to `debug` or `info` for logging (disabled by default). |

### 🏗️ Build & Run

**Local Execution:**

```bash
# Run with defaults
cargo run -p pavis-benchkit --bin bench-upstream

# Run with custom config
PORT=9090 FIXED_BYTES=1024 cargo run -p pavis-benchkit --bin bench-upstream
```

**Docker:**

The binary is packaged as a distroless container for minimal footprint.

```bash
# Build image
docker build -f crates/pavis-benchkit/Dockerfile -t pavis-bench-upstream .

# Run container
docker run -p 8000:8000 -e WORKER_THREADS=4 pavis-bench-upstream
```

## 📐 Design Principles

1.  **Zero Allocation**: Response bodies are `Arc<Bytes>` pre-allocated at startup.
2.  **No Magic**: No automatic middleware, compression, or header mutation. What you request is exactly what you get.
3.  **HTTP/1.1 Focus**: Optimized for HTTP/1.1 keepalive reuse, which is the primary protocol for backend connections in most proxy setups.
4.  **Observability Isolation**: Metrics generation is separated from the critical path to avoid observer effect during load testing.

## 📚 References

- **[Benchmark Methodology](../../bench/METHODOLOGY.md)**: How this backend is used in scientific benchmarks.
- **[Benchmark README](../../bench/README.md)**: Full suite documentation.

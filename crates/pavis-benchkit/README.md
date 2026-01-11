# Pavis Benchkit & bench-upstream

**Pavis Benchkit** is a specialized utility crate providing:
1. **bench-upstream**: A high-performance, deterministic HTTP backend for proxy benchmarks
2. **bench-loadgen**: A minimal open-loop HTTP load generator for latency testing

It is designed to eliminate "backend noise" (application runtime overhead, GC pauses, dynamic allocations) when measuring proxy latency and throughput.

---

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

---

## ⚡ bench-loadgen

`bench-loadgen` is a **purpose-built** open-loop HTTP load generator designed exclusively for Pavis latency benchmarks.

### Why This Tool Exists

**Problem**: Existing load generators (wrk2, hey, etc.) are either:
- Difficult to build on macOS (wrk2 + LuaJIT issues)
- Closed-source or complex to integrate
- General-purpose tools with features we don't need

**Solution**: A minimal, Rust-based open-loop load generator with:
- **Model equivalence** to wrk2 (not feature parity)
- Trivial cross-compilation (macOS, Linux, ARM, x86)
- Single-purpose design: latency benchmarking only
- Deterministic, reproducible output

### What This Is NOT

This is **NOT** a general-purpose load testing framework. It lacks:
- ❌ HTTP/2, gRPC, TLS support
- ❌ Scripting (Lua/JS/DSL)
- ❌ Multi-phase workloads
- ❌ Rate ramping or dynamic scenarios
- ❌ Distributed mode
- ❌ Interactive dashboards

If you need these features, use wrk2, k6, Gatling, or similar tools.

### Key Characteristics

1. **Open-Loop Semantics**: Request issuance follows a fixed schedule, independent of response times
   - Avoids coordinated omission bias
   - Models real-world load patterns (e.g., user arrivals)

2. **Deterministic**: HTTP/1.1 keepalive only, fixed headers, no dynamic payloads

3. **Low Overhead**: Minimal per-request allocations, lock-free hot paths

4. **Compatible Output**: JSON format matches existing `bench/cases/*.sh` expectations

### Usage

```bash
# Build
cargo build -p pavis-benchkit --bin bench-loadgen --release

# Run a 30-second test at 10,000 RPS with 500 connections
./target/release/bench-loadgen \
  --url http://localhost:8080/fixed \
  --rate 10000 \
  --duration 30 \
  --connections 500 \
  --timeout 2 \
  --output summary.json
```

**Required Flags**:
- `--url`: Target URL (HTTP/1.1 only, no TLS)
- `--rate`: Target requests per second (integer)
- `--duration`: Test duration in seconds
- `--connections`: Number of concurrent connections

**Optional Flags**:
- `--timeout`: Request timeout in seconds (default: 2)
- `--output`: Output file path (default: stdout)

### Output Format

```json
{
  "loadgen": "bench-loadgen",
  "load_type": "open-loop",
  "target_rps": 10000,
  "duration_s": 30,
  "connections": 500,
  "requests_scheduled": 300000,
  "dropped": 0,
  "requests_sent": 300000,
  "requests_ok": 299800,
  "errors": 200,
  "achieved_rps": 9993.4,
  "latency_ms": {
    "p50": 2.1,
    "p90": 3.4,
    "p99": 8.7
  }
}
```

**Field Semantics**:
- `requests_scheduled`: Total requests scheduled on the absolute time axis (target_rps × duration_s)
- `dropped`: Requests dropped due to saturation (all concurrency slots full)
- `requests_sent`: Requests actually sent to the network
- `requests_ok`: Successful responses (2xx/3xx status)
- `errors`: Failures (timeout, connection error, HTTP error status)
- `achieved_rps`: Successful requests per second (requests_ok / duration_s)

**Interpreting Results**:
- `dropped = 0`: Target RPS is sustainable with given concurrency
- `dropped > 0`: System is saturated, target RPS not achievable (this is working as designed - saturation is observable)

### Architecture

**Scheduler (Open-Loop Engine)**:
- Computes deadline for each request: `deadline_nanos = (i * 1_000_000_000) / rate`
- Uses `sleep_until(deadline)` for drift-free precision scheduling
- Issues request tokens independent of response completion
- **NEVER blocks** waiting for workers or responses
- Uses semaphore `try_acquire_owned()` (non-blocking) for concurrency control
- When all concurrency slots full → **drops request** and continues
- This guarantees true open-loop behavior (no feedback loop from responses to scheduling)

**Concurrency Control**:
- Semaphore with `connections` permits represents max in-flight requests
- Non-blocking try_acquire ensures scheduler never waits for workers
- Dropped requests make saturation observable in output
- This differs fundamentally from closed-loop tools that block until capacity available

**Worker Pool**:
- Workers are spawned dynamically when concurrency permits are available
- Each worker executes one HTTP request then exits
- HTTP client is shared (connection pooling, HTTP/1.1 keepalive)
- Latency measured from send time → response fully read
- Errors tracked separately (timeout, connection, HTTP errors)

**Statistics**:
- Latency samples stored in 16 sharded vectors (minimizes lock contention at high RPS)
- Round-robin shard selection using atomic counter
- Percentiles computed after test completion (not per-request)
- Atomic counters for requests scheduled/dropped/sent/ok/errors

### Design Rationale

**Why Not Use wrk2?**
- wrk2 is excellent but hard to build on macOS (LuaJIT + OpenSSL issues)
- bench-loadgen compiles trivially on all platforms

**Why Not Use wrk for Latency Tests?**
- wrk is closed-loop (coordinated omission bias)
- Latency numbers are inflated when backend slows down

**Why Not a Docker Container?**
- Container overhead adds latency noise
- Native binary eliminates virtualization layer
- Cross-compilation is straightforward in Rust

**Why Model Equivalence, Not Feature Parity?**
- We need open-loop semantics and percentile stats
- We don't need Lua scripting, multi-protocol support, etc.
- Smaller scope = simpler, more maintainable code

### Comparison: wrk2 vs bench-loadgen

| Feature | wrk2 | bench-loadgen |
|---------|------|---------------|
| Open-loop | ✅ | ✅ |
| Latency percentiles | ✅ | ✅ (p50, p90, p99) |
| HTTP/1.1 | ✅ | ✅ |
| HTTP/2 | ✅ | ❌ |
| TLS | ✅ | ❌ |
| Lua scripting | ✅ | ❌ |
| macOS build | ⚠️ Difficult | ✅ Trivial |
| JSON output | ❌ | ✅ |
| Docker-friendly | ⚠️ | ✅ |

### Limitations

- **HTTP/1.1 only**: No HTTP/2, gRPC, or TLS
- **Fixed headers**: No dynamic request generation
- **Percentiles only**: No full histogram export (HdrHistogram)
- **Single URL**: No multi-endpoint testing
- **No rate ramping**: Fixed rate for entire duration

These are intentional design choices to keep the tool simple and focused.

---

## 📐 Design Principles

1.  **Zero Allocation**: Response bodies are `Arc<Bytes>` pre-allocated at startup.
2.  **No Magic**: No automatic middleware, compression, or header mutation. What you request is exactly what you get.
3.  **HTTP/1.1 Focus**: Optimized for HTTP/1.1 keepalive reuse, which is the primary protocol for backend connections in most proxy setups.
4.  **Observability Isolation**: Metrics generation is separated from the critical path to avoid observer effect during load testing.

---

## 📚 References

- **[Benchmark Methodology](../../bench/METHODOLOGY.md)**: How this backend is used in scientific benchmarks.
- **[Benchmark README](../../bench/README.md)**: Full suite documentation.


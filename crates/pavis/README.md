# pavis

The Pavis runtime - a high-performance L7 proxy built on Pingora.

## Purpose

`pavis` is the data plane execution engine that interprets precompiled `.pvs` configuration artifacts and handles production traffic. It enforces the Frozen Data Plane model by accepting only validated binary configurations.

## Responsibilities

- Loading and applying `.pvs` configuration artifacts
- Managing proxy server lifecycle and listeners
- Routing requests via deterministic matching engine
- Managing upstream clusters and load balancing
- Atomic hot-reloading without dropping connections
- Environment validation (file readability and port availability) before applying configs
- Optional: Polling remote relay for configuration updates

## Build Requirements

The runtime uses Pingora's OpenSSL backend only; rustls is not supported or tested in CI.

## Module Structure

- **`app`**: Main entry point and Pingora server setup
- **`load`**: Configuration loading with `.pvs` enforcement
- **`router`**: Request matching engine (hybrid exact/prefix/regex)
- **`state`**: Global runtime state with atomic hot-reload (`ArcSwap`)
- **`proxy`**: Pingora integration layer
- **`upstream`**: Backend cluster and endpoint management
- **`agent`**: Background worker for remote configuration updates
- **`telemetry`**: Metrics, access logs, and distributed tracing
- **`validate_env`**: Runtime environment checks for file paths and ports

## Public API

The runtime exposes a read-only admin API (see specifications for details):
- `GET /health` - Liveness probe
- `GET /stats` - Runtime statistics

## Related Documentation

- **API Specification**: See [`docs/api/runtime-admin.md`](../../docs/api/runtime-admin.md)
- **Operations Guide**: See [`docs/operations/runtime.md`](../../docs/operations/runtime.md)
- **Recovery Procedures**: See [`docs/operations/recovery.md`](../../docs/operations/recovery.md)
- **Architecture**: See [`/ARCHITECTURE.md`](../../ARCHITECTURE.md)
- **Design Philosophy**: See [`docs/design.md`](../../docs/design.md)

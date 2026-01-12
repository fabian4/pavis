# Phase 5: Observability Implementation Plan

**Status**: Draft
**Reference**: [ROADMAP.md](../../ROADMAP.md) - Phase 4 (Next Phase)

This document outlines the technical implementation plan for "Phase 5: Observability," focusing on Prometheus metrics, structured access logging, distributed tracing (OTLP), and internal runtime statistics.

---

## 1. Prometheus Metrics

**Goal**: Exporter with request, connection, and upstream dimensions.

### Implementation Tasks

1.  **Core Metrics Definition**:
    - **Request Metrics**: `pavis_http_requests_total{method, path, status, upstream}`, `pavis_http_request_duration_seconds{method, path, status, upstream}` (Histogram).
    - **Connection Metrics**: `pavis_connections_active`, `pavis_connections_total`.
    - **Upstream Metrics**: `pavis_upstream_requests_total{upstream, endpoint, status}`, `pavis_upstream_request_duration_seconds{upstream, endpoint}`.

2.  **Metrics Architecture**:
    - Use the `prometheus` or `metrics-exporter-prometheus` crate.
    - Implement `MetricsWorker` as a `pingora::services::Service`.
    - The worker will listen on the `addr` defined in `Telemetry.metrics` config.
    - **Non-blocking updates**: Use atomic counters or `metrics` macros in the hot path (`request_filter`, `upstream_peer`, `logging`).

3.  **Integration**:
    - Register the `MetricsWorker` in `main.rs` if `metrics` is enabled.
    - Inject a handle to the metrics registry/exporter into the `Proxy` and `Telemetry` structs.

---

## 2. Refined Access Logging

**Goal**: Configurable JSON/Text output with extended metadata.

### Implementation Tasks

1.  **Structured Output (JSON)**:
    - Add a `format` field to `AccessLogPolicy` in `pavis-core` (or default to JSON if preferred for modern environments).
    - Use `serde_json` in `AccessLogWorker` to serialize `LogEntry`.
    - **Text Format**: Maintain the existing space-separated format for CLI-friendly debugging.

2.  **Extended Metadata**:
    - Capture and log:
        - `X-Pavis-Generated-At`: Trace the configuration version.
        - `X-Request-Id`: Ensure propagation and logging.
        - `User-Agent`: For client analysis.
        - `Upstream-Latency`: Time spent waiting for backend response.

3.  **Performance**:
    - Ensure the `mpsc` channel buffer size is configurable if needed, but maintain the lossy `try_send` behavior to protect the data plane.

---

## 3. Distributed Tracing (OTLP)

**Goal**: OpenTelemetry integration for request tracing.

### Implementation Tasks

1.  **OpenTelemetry Integration**:
    - Use `opentelemetry`, `opentelemetry-otlp`, and `tracing-opentelemetry` crates.
    - Initialize the tracer based on `Telemetry.tracing` configuration (Provider: OTLP).

2.  **Span Lifecycle**:
    - **Start Span**: In `request_filter`, start a new span named after the route or method/path.
    - **Propagation**: Inject tracing headers (`b3`, `traceparent`) into upstream requests in `upstream_request_filter`.
    - **End Span**: In `logging`, close the span and record final status/latency.

3.  **Sampling**:
    - Implement `SampleRate` (e.g., 1/1000) as defined in `pavis-core` to control volume.

---

## 4. Internal Runtime Statistics

**Goal**: Telemetry for the proxy's own health and state.

### Implementation Tasks

1.  **Gauges**:
    - `pavis_runtime_config_version`: A gauge representing the version (timestamp or partial Git SHA).
    - `pavis_runtime_reload_count_total`: Number of successful hot reloads.
    - `pavis_runtime_reload_last_timestamp`: Unix timestamp of the last reload.
    - `pavis_runtime_config_size_bytes`: Size of the currently loaded PVS artifact.

2.  **Implementation**:
    - Update these gauges in `state.rs` whenever a new `RuntimeConfig` is swapped via `ArcSwap`.
    - Expose via the same Prometheus endpoint as traffic metrics.

---

## 5. Testing & Verification

### Unit Tests
- `telemetry/access_log.rs`: Verify JSON formatting logic.
- `telemetry/metrics.rs`: Verify counter increments during simulated requests.

### E2E Tests
- `tests/suites/pavis/70_observability_metrics.sh`:
    - Start Pavis with metrics enabled.
    - Send N requests.
    - `curl http://pavis:9090/metrics` and grep for `pavis_http_requests_total`.
- `tests/suites/pavis/71_observability_logging.sh`:
    - Verify logs are written to a file in JSON format.
    - Assert specific fields are present (e.g., `request_id`, `upstream`).

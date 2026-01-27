# Metrics & Observability Reference

## 1. Overview

### Metrics scraping (Pavis)
- **Exporter**: Prometheus text format via `metrics_exporter_prometheus`.
- **Bind address**: `telemetry.metrics = Metrics::Enabled { addr }` in runtime config (core type in `crates/pavis-core/src/runtime/telemetry.rs`, wired in `crates/pavis/src/telemetry.rs`).
- **Server implementation**: `crates/pavis/src/telemetry/metrics.rs::MetricsWorker` binds a raw TCP listener and responds with Prometheus text to any HTTP request (path is ignored). For convention, scrape with `GET /metrics` on the configured addr.
- **Important note**: If the exporter fails to initialize, metrics are disabled and the worker logs a warning.

### Metrics scraping (Relay)
- **Endpoint**: `GET /v1/metrics` on the Relay HTTP server (`crates/pavis-relay/src/routes.rs`).
- **Response**: Prometheus text with a small set of relay counters and a version gauge, assembled in `crates/pavis-relay/src/handlers.rs::get_metrics`.
- **Config knob**: `metrics.prometheus_bind` exists in relay config types (`crates/pavis-relay/src/config/types.rs`) but is not wired to a separate listener in code. Metrics are served on the main relay HTTP listener only.

### Logging and tracing knobs (Pavis)
- **Structured logs**: `tracing_subscriber` with EnvFilter + fmt layer in `crates/pavis/src/main.rs`.
  - Default filter uses `telemetry.level` from config.
  - `RUST_LOG` overrides via `EnvFilter::try_from_default_env()`.
  - Pingora log level uses `telemetry.pingora` and is added as directives for `pingora` and `pingora_core`.
- **Access log**: configured via `telemetry.access_log` (Disabled/Stdout/File) and emitted by `crates/pavis/src/telemetry/access_log.rs` (JSON per line).
- **Tracing**: OpenTelemetry OTLP exporter is set up when `telemetry.tracing = TracingPolicy::Enabled { provider, sampling, endpoint }` and initialized in `crates/pavis/src/telemetry/tracing.rs::TracingService` (and on config reload via `crates/pavis/src/telemetry/tracing.rs::maybe_init_tracing`). Only OTLP exporter is implemented; other providers are not wired.

### Health / admin endpoints
- **Pavis admin API** (`crates/pavis/src/admin.rs`):
  - `GET /health` → `{"status":"healthy"}`
  - `GET /stats` → `{"version":"...","uptime_seconds":...,"listeners":...,"upstreams":...,"routes":...}`
  - Bound by `admin` config (`pavis_core::AdminConfig`) when enabled.
- **Relay** (`crates/pavis-relay/src/routes.rs` + `handlers.rs`):
  - `GET /health` → `ok` (200)
  - `GET /ready` → `ready` or `no artifact` (503 if not ready)
  - `GET /v1/status` → JSON status (see `handlers.rs::get_status`)

---

## 2. Metrics Index (table)

**Legend**: “Def” = definition/metric object; “Emit” = primary call-sites.

| Metric name | Type | Unit | Labels | Meaning | Def / Emit (primary) |
|---|---|---|---|---|---|
| `pavis_route_match_attempts_total` | counter | requests | `result` (`matched`/`no_match`) | Total route match attempts | Def: `crates/pavis/src/telemetry/metrics.rs::MetricsHandle::record_route_match`; Emit: `crates/pavis/src/proxy/service.rs::request_filter` |
| `pavis_route_match_predicate_failures_total` | counter | predicate failures | `predicate_type` (`path`/`method`/`header`) | Failed predicate checks during routing | Def: `metrics.rs::record_route_match`; Emit: `proxy/service.rs::request_filter` |
| `pavis_route_match_predicate_evaluations_total` | counter | predicate evals | `operator` (`exact`/`prefix`/`regex`/`present`/`absent`) | Predicate evaluations per operator | Def: `metrics.rs::record_route_match`; Emit: `proxy/service.rs::request_filter` |
| `pavis_route_match_regex_input_too_large_total` | counter | rejections | (none) | Regex input rejected due to size limits | Def: `metrics.rs::record_route_match`; Emit: `proxy/service.rs::request_filter` |
| `pavis_http_requests_total` | counter | requests | `method`, `route`, `status`, `upstream` | Total HTTP requests served by Pavis | Def: `metrics.rs::record_request`; Emit: `proxy/service.rs::logging` |
| `pavis_http_request_duration_seconds` | histogram | seconds | `method`, `route`, `status`, `upstream` | End-to-end request latency | Def: `metrics.rs::record_request`; Emit: `proxy/service.rs::logging` |
| `pavis_upstream_requests_total` | counter | requests | `upstream`, `status` | Requests issued to upstreams | Def: `metrics.rs::record_upstream_request`; Emit: `proxy/service.rs::logging` |
| `pavis_upstream_request_duration_seconds` | histogram | seconds | `upstream` | Upstream latency | Def: `metrics.rs::record_upstream_request`; Emit: `proxy/service.rs::logging` |
| `pavis_http_inflight_requests` | gauge | requests | (none) | In-flight HTTP requests | Def: `metrics.rs::increment_active_connections/decrement_active_connections`; Emit: `proxy/service.rs::request_filter` / `proxy/service.rs::logging` |
| `pavis_connections_total` | counter | connections | (none) | Total connections observed | Def: `metrics.rs::increment_active_connections`; Emit: `proxy/service.rs::request_filter` |
| `pavis_upstream_pool_queue_capacity` | gauge | requests | `upstream` | Pool queue capacity | Def/Emit: `crates/pavis/src/upstream/cluster.rs::record_queue_capacity_metric` (`PoolController::new`) |
| `pavis_upstream_pool_queue_depth` | gauge | requests | `upstream` | Current queue depth | Def/Emit: `cluster.rs::record_queue_depth_metric` (`PoolLimiter::acquire`, `finish_queue_wait`) |
| `pavis_upstream_pool_size` | gauge | connections | `upstream` | Active pool size | Def/Emit: `cluster.rs::record_pool_size_metric` (`PoolLimiter::start_pool_use/finish_pool_use`) |
| `pavis_upstream_pool_rejections_total` | counter | rejections | `upstream`, `reason` (`queue_full`/`queue_timeout`) | Queue/pool rejections | Def/Emit: `cluster.rs::PoolController::record_rejection` |
| `pavis_upstream_pool_key_cardinality_approx` | gauge | keys | `upstream` | Approx cardinality of pool reuse keys (capped) | Def: `metrics.rs::record_pool_key_cardinality`; Emit: `proxy/service.rs::get_peer` (via `PoolKeyCardinalityTracker`) |
| `pavis_upstream_connection_reused_total` | counter | connections | `upstream` | Reused upstream connections | Def: `metrics.rs::record_connection_reused`; Emit: `proxy/service.rs::connected_to_upstream` |
| `pavis_upstream_connection_new_total` | counter | connections | `upstream`, `reason` (currently `new_connection`) | New upstream connections created | Def: `metrics.rs::record_connection_new`; Emit: `proxy/service.rs::connected_to_upstream` |
| `pavis_runtime_config_version` | gauge | version | `version` | Current config version (set to 1.0 for active version label) | Def: `metrics.rs::update_config_stats`; Emit: `agent/worker/agent.rs::record_config_stats` |
| `pavis_runtime_config_size_bytes` | gauge | bytes | (none) | Size of current runtime config | Def: `metrics.rs::update_config_stats`; Emit: `agent/worker/agent.rs::record_config_stats` |
| `pavis_runtime_reload_last_timestamp` | gauge | unix seconds | (none) | Last config reload time (epoch seconds) | Def: `metrics.rs::update_config_stats`; Emit: `agent/worker/agent.rs::record_config_stats` |
| `pavis_config_validation_total` | counter | validations | `result` (`ok`/`fail`), `reason` (`parse`/`version`/`runtime`/`semantic`) | Config validation outcomes | Def: `metrics.rs::record_config_validation`; Emit: `agent/worker/agent.rs::record_validation` |
| `pavis_config_apply_total` | counter | applies | `result` (`ok`/`fail`) | Config apply outcomes | Def: `metrics.rs::record_config_apply`; Emit: `agent/worker/agent.rs::record_apply` |
| `pavis_upstream_retries_total` | counter | retries | `upstream`, `reason` (`status_code`/`connect_timeout`/`read_timeout`/`per_try_timeout`/`pool_full`/`connect_error`), `attempt` | Retry attempts | Def: `metrics.rs::record_retry`; Emit: `retry.rs::RetryContext::next_attempt` |
| `pavis_upstream_retry_outcome_total` | counter | retries | `upstream`, `outcome` (`success`/`exhausted`) | Final retry outcome | Def: `metrics.rs::record_retry_outcome`; Emit: `retry.rs::RetryContext::record_outcome` |
| `pavis_upstream_retry_body_buffer_size_bytes` | histogram | bytes | `upstream` | Buffered body size for retry replay | Def: `metrics.rs::record_retry_body_buffered`; Emit: `retry.rs` body buffer path |
| `pavis_telemetry_metrics_label_dropped_total` | counter | drops | (none) | Count of requests where labels were dropped (no matched route) | Def: `metrics.rs::record_metrics_label_dropped`; Emit: `proxy/service.rs::logging` |
| `pavis_telemetry_access_log_dropped_total` | counter | drops | (none) | Access log drops due to backpressure | Def: `metrics.rs::record_access_log_dropped`; Emit: `telemetry/access_log.rs::AccessLog::log` |
| `pavis_telemetry_tracing_export_errors_total` | counter | errors | (none) | Trace export errors (per export batch) | Def: `metrics.rs::record_tracing_export_error`; Emit: `telemetry/tracing.rs::MetricsSpanExporter::export` |
| `pavis_telemetry_tracing_spans_created_total` | counter | spans | (none) | Spans created (request-level, tracing enabled) | Def: `metrics.rs::record_span_created`; Emit: `telemetry/tracing.rs::SpanMetricsLayer::on_new_span` |
| `pavis_telemetry_tracing_spans_exported_total` | counter | spans | (none) | Spans exported (per export batch) | Def: `metrics.rs::record_span_exported`; Emit: `telemetry/tracing.rs::MetricsSpanExporter::export` |
| `pavis_runtime_reload_count_total` | counter | reloads | (none) | Config reload count | Def: `metrics.rs::increment_reload_count`; Emit: `agent/worker/agent.rs::apply_update` |
| `pavis_relay_version` | gauge | version | (none) | Relay current config version | Def/Emit: `crates/pavis-relay/src/handlers.rs::get_metrics` |
| `pavis_relay_publish_ok_total` | counter | publishes | (none) | Successful publishes | Def: `handlers.rs::get_metrics` (formatting); Emit: `runtime.rs::publish_auto`, `runtime.rs::publish_bytes` |
| `pavis_relay_publish_fail_total` | counter | publishes | (none) | Failed publishes | Def: `handlers.rs::get_metrics`; Emit: `handlers.rs::post_publish` |
| `pavis_relay_longpoll_wait_total` | counter | waits | (none) | Long-poll waits | Def: `handlers.rs::get_metrics`; Emit: `handlers.rs::get_config` |

---

## 3. Detailed Metric Specs

### `pavis_route_match_attempts_total`
- **Type/unit**: counter / requests
- **Labels**: `result` = `matched` or `no_match`
- **Semantics**: incremented once per request after routing attempt.
- **Cardinality notes**: low cardinality (2 values).
- **Definition**: `crates/pavis/src/telemetry/metrics.rs::MetricsHandle::record_route_match`
- **Primary emission**: `crates/pavis/src/proxy/service.rs::request_filter`
- **PromQL**:
  - `sum(rate(pavis_route_match_attempts_total[5m])) by (result)`
  - `sum(increase(pavis_route_match_attempts_total[1h]))`

### `pavis_route_match_predicate_failures_total`
- **Type/unit**: counter / predicate failures
- **Labels**: `predicate_type` = `path|method|header`
- **Semantics**: increments by the number of misses for each predicate type during route evaluation.
- **Cardinality notes**: low cardinality (3 values).
- **Definition**: `metrics.rs::record_route_match`
- **Emission**: `proxy/service.rs::request_filter`
- **PromQL**:
  - `sum(rate(pavis_route_match_predicate_failures_total[5m])) by (predicate_type)`
  - `increase(pavis_route_match_predicate_failures_total[1h])`

### `pavis_route_match_predicate_evaluations_total`
- **Type/unit**: counter / evaluations
- **Labels**: `operator` = `exact|prefix|regex|present|absent`
- **Semantics**: increments by the number of predicate evaluations per operator.
- **Cardinality notes**: low cardinality (5 values).
- **Definition**: `metrics.rs::record_route_match`
- **Emission**: `proxy/service.rs::request_filter`
- **PromQL**:
  - `sum(rate(pavis_route_match_predicate_evaluations_total[5m])) by (operator)`
  - `increase(pavis_route_match_predicate_evaluations_total[1h])`

### `pavis_route_match_regex_input_too_large_total`
- **Type/unit**: counter / rejections
- **Labels**: none
- **Semantics**: increments when a regex predicate rejects input for exceeding configured limits.
- **Cardinality notes**: none.
- **Definition**: `metrics.rs::record_route_match`
- **Emission**: `proxy/service.rs::request_filter`
- **PromQL**:
  - `rate(pavis_route_match_regex_input_too_large_total[5m])`
  - `increase(pavis_route_match_regex_input_too_large_total[1h])`

### `pavis_http_requests_total`
- **Type/unit**: counter / requests
- **Labels**: `method`, `route`, `status`, `upstream`
- **Semantics**: increments once per completed request with route match.
- **Cardinality notes**: high if `route` or `upstream` is unbounded. Use stable route patterns (not raw paths) and configured upstream names.
- **Definition**: `metrics.rs::record_request`
- **Emission**: `proxy/service.rs::logging`
- **PromQL**:
  - `sum(rate(pavis_http_requests_total[5m])) by (status)`
  - `sum(rate(pavis_http_requests_total[5m])) by (route)`
  - `sum(rate(pavis_http_requests_total[5m])) by (upstream)`

### `pavis_http_request_duration_seconds`
- **Type/unit**: histogram / seconds
- **Labels**: `method`, `route`, `status`, `upstream`
- **Semantics**: end-to-end request latency.
- **Cardinality notes**: same as `pavis_http_requests_total`.
- **Definition**: `metrics.rs::record_request`
- **Emission**: `proxy/service.rs::logging`
- **PromQL**:
  - `histogram_quantile(0.95, sum(rate(pavis_http_request_duration_seconds_bucket[5m])) by (le, route))`
  - `sum(rate(pavis_http_request_duration_seconds_sum[5m])) / sum(rate(pavis_http_request_duration_seconds_count[5m]))`

### `pavis_upstream_requests_total`
- **Type/unit**: counter / requests
- **Labels**: `upstream`, `status`
- **Semantics**: counts upstream requests for matched routes.
- **Cardinality notes**: status is bounded; upstreams should be stable.
- **Definition**: `metrics.rs::record_upstream_request`
- **Emission**: `proxy/service.rs::logging`
- **PromQL**:
  - `sum(rate(pavis_upstream_requests_total[5m])) by (upstream)`
  - `sum(rate(pavis_upstream_requests_total[5m])) by (status)`

### `pavis_upstream_request_duration_seconds`
- **Type/unit**: histogram / seconds
- **Labels**: `upstream`
- **Semantics**: duration from upstream start to response completion (or full request duration if upstream never started).
- **Cardinality notes**: bounded by upstream count.
- **Definition**: `metrics.rs::record_upstream_request`
- **Emission**: `proxy/service.rs::logging`
- **PromQL**:
  - `histogram_quantile(0.95, sum(rate(pavis_upstream_request_duration_seconds_bucket[5m])) by (le, upstream))`
  - `sum(rate(pavis_upstream_request_duration_seconds_sum[5m])) by (upstream) / sum(rate(pavis_upstream_request_duration_seconds_count[5m])) by (upstream)`

### `pavis_http_inflight_requests`
- **Type/unit**: gauge / requests
- **Labels**: none
- **Semantics**: current number of in-flight requests.
- **Definition**: `metrics.rs::increment_active_connections/decrement_active_connections`
- **Emission**: `proxy/service.rs::request_filter` and `proxy/service.rs::logging`
- **PromQL**:
  - `pavis_http_inflight_requests`
  - `max_over_time(pavis_http_inflight_requests[5m])`

### `pavis_connections_total`
- **Type/unit**: counter / connections
- **Labels**: none
- **Semantics**: increments when a request is accepted into the proxy filter (approx connection volume).
- **Definition**: `metrics.rs::increment_active_connections`
- **Emission**: `proxy/service.rs::request_filter`
- **PromQL**:
  - `rate(pavis_connections_total[5m])`
  - `increase(pavis_connections_total[1h])`

### `pavis_upstream_pool_queue_capacity`
- **Type/unit**: gauge / requests
- **Labels**: `upstream`
- **Semantics**: configured queue capacity for upstream pool.
- **Definition/Emission**: `crates/pavis/src/upstream/cluster.rs::record_queue_capacity_metric` (called in `PoolController::new`)
- **PromQL**:
  - `max(pavis_upstream_pool_queue_capacity) by (upstream)`

### `pavis_upstream_pool_queue_depth`
- **Type/unit**: gauge / requests
- **Labels**: `upstream`
- **Semantics**: current queued requests waiting for pool permits.
- **Definition/Emission**: `cluster.rs::record_queue_depth_metric` (called in `PoolLimiter::acquire` and `finish_queue_wait`)
- **PromQL**:
  - `avg_over_time(pavis_upstream_pool_queue_depth[5m]) by (upstream)`
  - `max_over_time(pavis_upstream_pool_queue_depth[5m]) by (upstream)`

### `pavis_upstream_pool_size`
- **Type/unit**: gauge / connections
- **Labels**: `upstream`
- **Semantics**: current active connections in the upstream pool.
- **Definition/Emission**: `cluster.rs::record_pool_size_metric` (called in `PoolLimiter::start_pool_use` / `finish_pool_use`)
- **PromQL**:
  - `avg_over_time(pavis_upstream_pool_size[5m]) by (upstream)`
  - `max_over_time(pavis_upstream_pool_size[5m]) by (upstream)`

### `pavis_upstream_pool_rejections_total`
- **Type/unit**: counter / rejections
- **Labels**: `upstream`, `reason` (`queue_full|queue_timeout`)
- **Semantics**: number of requests rejected by pool limits or queue timeout.
- **Definition/Emission**: `cluster.rs::PoolController::record_rejection`
- **PromQL**:
  - `sum(rate(pavis_upstream_pool_rejections_total[5m])) by (upstream, reason)`
  - `increase(pavis_upstream_pool_rejections_total[1h])`

### `pavis_upstream_pool_key_cardinality_approx`
- **Type/unit**: gauge / keys
- **Labels**: `upstream`
- **Semantics**: approximate unique count of pool reuse keys over a 60s TTL. If saturated, it reports `POOL_KEY_CARDINALITY_CAP + 1` (cap defined as 1024 in `metrics.rs`).
- **Cardinality notes**: This metric guards against cardinality blow-ups; treat `>1024` as “saturated”.
- **Definition**: `metrics.rs::PoolKeyCardinalityTracker` + `record_pool_key_cardinality`
- **Emission**: `proxy/service.rs::get_peer`
- **PromQL**:
  - `max_over_time(pavis_upstream_pool_key_cardinality_approx[5m]) by (upstream)`

### `pavis_upstream_connection_reused_total`
- **Type/unit**: counter / connections
- **Labels**: `upstream`
- **Semantics**: count of reused upstream connections.
- **Definition**: `metrics.rs::record_connection_reused`
- **Emission**: `proxy/service.rs::connected_to_upstream`
- **PromQL**:
  - `sum(rate(pavis_upstream_connection_reused_total[5m])) by (upstream)`

### `pavis_upstream_connection_new_total`
- **Type/unit**: counter / connections
- **Labels**: `upstream`, `reason` (currently `new_connection`)
- **Semantics**: new upstream connection creations.
- **Definition**: `metrics.rs::record_connection_new`
- **Emission**: `proxy/service.rs::connected_to_upstream`
- **PromQL**:
  - `sum(rate(pavis_upstream_connection_new_total[5m])) by (upstream)`

### `pavis_runtime_config_version`
- **Type/unit**: gauge / version
- **Labels**: `version`
- **Semantics**: the active version label is set to 1.0. Multiple labels may remain if not cleaned by the exporter.
- **Definition**: `metrics.rs::update_config_stats`
- **Emission**: `agent/worker/agent.rs::record_config_stats`
- **PromQL**:
  - `max(pavis_runtime_config_version) by (version)`

### `pavis_runtime_config_size_bytes`
- **Type/unit**: gauge / bytes
- **Labels**: none
- **Semantics**: size of current runtime config.
- **Definition/Emission**: `metrics.rs::update_config_stats` via `agent/worker/agent.rs::record_config_stats`
- **PromQL**:
  - `pavis_runtime_config_size_bytes`

### `pavis_runtime_reload_last_timestamp`
- **Type/unit**: gauge / unix seconds
- **Labels**: none
- **Semantics**: epoch seconds of last config stats update.
- **Definition/Emission**: `metrics.rs::update_config_stats` via `agent/worker/agent.rs::record_config_stats`
- **PromQL**:
  - `pavis_runtime_reload_last_timestamp`

### `pavis_config_validation_total`
- **Type/unit**: counter / validations
- **Labels**: `result` (`ok|fail`), `reason` (`parse|version|runtime|semantic`)
- **Semantics**: config validation outcomes (reason mapping in `agent/worker/agent.rs::classify_validation_error`).
- **Definition**: `metrics.rs::record_config_validation`
- **Emission**: `agent/worker/agent.rs::record_validation`
- **PromQL**:
  - `sum(rate(pavis_config_validation_total[5m])) by (result, reason)`

### `pavis_config_apply_total`
- **Type/unit**: counter / applies
- **Labels**: `result` (`ok|fail`)
- **Semantics**: config apply outcomes.
- **Definition**: `metrics.rs::record_config_apply`
- **Emission**: `agent/worker/agent.rs::record_apply`
- **PromQL**:
  - `sum(rate(pavis_config_apply_total[5m])) by (result)`

### `pavis_upstream_retries_total`
- **Type/unit**: counter / retries
- **Labels**: `upstream`, `reason`, `attempt`
- **Semantics**: increments per retry attempt (attempt is 1-indexed).
- **Cardinality notes**: `attempt` increases cardinality; keep max attempts small.
- **Definition**: `metrics.rs::record_retry`
- **Emission**: `retry.rs::RetryContext::next_attempt`
- **PromQL**:
  - `sum(rate(pavis_upstream_retries_total[5m])) by (upstream, reason)`

### `pavis_upstream_retry_outcome_total`
- **Type/unit**: counter / retries
- **Labels**: `upstream`, `outcome` (`success|exhausted`)
- **Semantics**: final retry outcome per request.
- **Definition**: `metrics.rs::record_retry_outcome`
- **Emission**: `retry.rs::RetryContext::record_outcome`
- **PromQL**:
  - `sum(rate(pavis_upstream_retry_outcome_total[5m])) by (upstream, outcome)`

### `pavis_upstream_retry_body_buffer_size_bytes`
- **Type/unit**: histogram / bytes
- **Labels**: `upstream`
- **Semantics**: size of buffered request bodies for retries.
- **Definition**: `metrics.rs::record_retry_body_buffered`
- **Emission**: `retry.rs` body buffer path
- **PromQL**:
  - `histogram_quantile(0.95, sum(rate(pavis_upstream_retry_body_buffer_size_bytes_bucket[5m])) by (le, upstream))`

### `pavis_telemetry_metrics_label_dropped_total`
- **Type/unit**: counter / drops
- **Labels**: none
- **Semantics**: increments when request labels can’t be recorded because no route matched.
- **Definition**: `metrics.rs::record_metrics_label_dropped`
- **Emission**: `proxy/service.rs::logging` (when `RoutePattern::NotMatched`)
- **PromQL**:
  - `rate(pavis_telemetry_metrics_label_dropped_total[5m])`

### `pavis_telemetry_access_log_dropped_total`
- **Type/unit**: counter / drops
- **Labels**: none
- **Semantics**: intended to count dropped access log entries on backpressure.
- **Definition**: `metrics.rs::record_access_log_dropped`
- **Emission sites**: `telemetry/access_log.rs::AccessLog::log` (only on `try_send` backpressure)
- **Emission**: **No runtime call-site** (not currently incremented).

### `pavis_telemetry_tracing_export_errors_total`
- **Type/unit**: counter / errors
- **Labels**: none
- **Semantics**: counts tracing export errors per failed export batch.
- **Definition**: `metrics.rs::record_tracing_export_error`
- **Emission sites**: `telemetry/tracing.rs::MetricsSpanExporter::export` error path
- **Emission**: **No runtime call-site**.

### `pavis_telemetry_tracing_spans_created_total`
- **Type/unit**: counter / spans
- **Labels**: none
- **Semantics**: intended to count spans created.
- **Definition**: `metrics.rs::record_span_created`
- **Emission sites**: `telemetry/tracing.rs::SpanMetricsLayer::on_new_span` (tracing enabled only)
- **Emission**: **No runtime call-site**.

### `pavis_telemetry_tracing_spans_exported_total`
- **Type/unit**: counter / spans
- **Labels**: none
- **Semantics**: intended to count spans exported.
- **Definition**: `metrics.rs::record_span_exported`
- **Emission sites**: `telemetry/tracing.rs::MetricsSpanExporter::export` success path
- **Emission**: **No runtime call-site**.

### `pavis_runtime_reload_count_total`
- **Type/unit**: counter / reloads
- **Labels**: none
- **Semantics**: counts successful config activations after swap and `/stats` visibility.
- **Definition**: `metrics.rs::increment_reload_count`
- **Emission sites**: `agent/worker/agent.rs::apply_update` (post-swap, config version visible)
- **Emission**: **No runtime call-site**.

### `pavis_relay_version`
- **Type/unit**: gauge / version
- **Labels**: none
- **Semantics**: current relay config version.
- **Definition/Emission**: `crates/pavis-relay/src/handlers.rs::get_metrics`
- **PromQL**:
  - `pavis_relay_version`

### `pavis_relay_publish_ok_total`
- **Type/unit**: counter / publishes
- **Labels**: none
- **Semantics**: successful publishes.
- **Definition**: `handlers.rs::get_metrics`
- **Emission**: `runtime.rs::publish_auto`, `runtime.rs::publish_bytes`
- **PromQL**:
  - `rate(pavis_relay_publish_ok_total[5m])`

### `pavis_relay_publish_fail_total`
- **Type/unit**: counter / publishes
- **Labels**: none
- **Semantics**: failed publishes.
- **Definition**: `handlers.rs::get_metrics`
- **Emission**: `handlers.rs::post_publish`
- **PromQL**:
  - `rate(pavis_relay_publish_fail_total[5m])`

### `pavis_relay_longpoll_wait_total`
- **Type/unit**: counter / waits
- **Labels**: none
- **Semantics**: number of long-poll wait loops entered.
- **Definition**: `handlers.rs::get_metrics`
- **Emission**: `handlers.rs::get_config`
- **PromQL**:
  - `rate(pavis_relay_longpoll_wait_total[5m])`

---

## 4. Diagnostic Playbooks (metric-driven)

### Is upstream pooling fragmented?
**Inspect**:
- `pavis_upstream_pool_key_cardinality_approx`
- `pavis_upstream_connection_new_total`
- `pavis_upstream_connection_reused_total`

**PromQL**:
- `max_over_time(pavis_upstream_pool_key_cardinality_approx[5m]) by (upstream)`
- `sum(rate(pavis_upstream_connection_new_total[5m])) by (upstream)`
- `sum(rate(pavis_upstream_connection_reused_total[5m])) by (upstream)`

**Interpretation**:
- Rising key cardinality and high new-connection rate suggest too many reuse keys (SNI/verify/cert changes) or fragmented upstreams.

### Are we reusing upstream connections?
**Inspect**:
- `pavis_upstream_connection_reused_total`
- `pavis_upstream_connection_new_total`

**PromQL**:
- `sum(rate(pavis_upstream_connection_reused_total[5m])) by (upstream)`
- `sum(rate(pavis_upstream_connection_new_total[5m])) by (upstream)`

**Interpretation**:
- New >> reused implies poor connection reuse (check TLS/SNI/verify mismatch, pool key cardinality, or low pool max).

### Are we queueing due to pool limits?
**Inspect**:
- `pavis_upstream_pool_queue_depth`
- `pavis_upstream_pool_queue_capacity`
- `pavis_upstream_pool_rejections_total`

**PromQL**:
- `max_over_time(pavis_upstream_pool_queue_depth[5m]) by (upstream)`
- `max(pavis_upstream_pool_queue_capacity) by (upstream)`
- `sum(rate(pavis_upstream_pool_rejections_total{reason="queue_full"}[5m])) by (upstream)`

**Interpretation**:
- Sustained queue depth near capacity and queue_full rejections indicate pool saturation.

### Are we dropping requests under saturation?
**Inspect**:
- `pavis_upstream_pool_rejections_total`
- `pavis_upstream_retry_outcome_total{outcome="exhausted"}`
- `pavis_http_requests_total` (5xx)

**PromQL**:
- `sum(rate(pavis_upstream_pool_rejections_total[5m])) by (upstream, reason)`
- `sum(rate(pavis_upstream_retry_outcome_total{outcome="exhausted"}[5m])) by (upstream)`
- `sum(rate(pavis_http_requests_total{status=~"5.."}[5m])) by (route)`

**Interpretation**:
- High rejection rates or exhausted retries correlate with 5xx spikes and indicate saturation or upstream failure.

### Is memory (RSS) growing under load?
**Inspect**:
- **No built-in Pavis RSS metric is exported in this repository.**

**PromQL**:
- Not applicable with built-in metrics. Use an external process/container metrics source and correlate with `pavis_http_inflight_requests` and request rates.

**Interpretation**:
- If external RSS grows with steady traffic while inflight requests are stable, suspect leaks or unbounded buffers.

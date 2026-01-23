# Pavis Runtime Configuration Reference

This document provides a normative, exhaustive reference for the Pavis data plane YAML configuration. It is derived strictly from the codec and runtime implementation in the codebase.

## Overview

The configuration flows through three validation layers: **Codec** (format/static checks), **Core** (semantic invariants), and **Runtime** (environment checks). After validation, it is converted into an immutable `RuntimeConfig` artifact used by the proxy engine.

### Top-Level Schema

- [`listeners[]`](#listeners) - Inbound entry points.
- [`telemetry`](#telemetry) - Global observability.
- [`upstreams[]`](#upstreams) - Backend clusters and endpoints.
- [`routes[]`](#routes) - Virtual hosts and request routing.

---

## Listeners <a id="listeners"></a>

Defines how the proxy accepts inbound traffic.

### `listeners[].name`
- **Type**: `string`
- **Required**: Yes
- **Validation**: Must be unique across all listeners.
- **Runtime Effect**: Used for logging and metrics identification.

### `listeners[].address`
- **Type**: `string`
- **Required**: Yes
- **Allowed values**: A valid socket address (IP:PORT), e.g., `"0.0.0.0:8080"`.
- **Validation**: Port must be unique across listeners, admin, and metrics.
- **Runtime Effect**: Binds the proxy to this address.

### `listeners[].workers`
- **Type**: `integer`
- **Required**: Optional
- **Default**: Derived from CPU cores (Auto).
- **Validation**: Must be `> 0` and `< 65536`.
- **Runtime Effect**: Sets the number of worker threads for this listener.

### `listeners[].tls`
- **Type**: `object`
- **Required**: Optional
- **Runtime Effect**: Enables TLS termination if present.

### `listeners[].tls.cert_path`
- **Type**: `string`
- **Required**: Yes (if `tls` is set)
- **Validation**: Runtime validates the path exists and is readable.
- **Runtime Effect**: Path to the PEM certificate file.

### `listeners[].tls.key_path`
- **Type**: `string`
- **Required**: Yes (if `tls` is set)
- **Validation**: Runtime validates the path exists and is readable.
- **Runtime Effect**: Path to the PEM private key file.

### `listeners[].tls.client_auth`
- **Type**: `enum / object`
- **Required**: Optional
- **Allowed Values**:
  - `disabled`: Unit variant. No client auth.
  - `optional: { ca_path: "..." }`: Client certificate requested but not required.
  - `required: { ca_path: "..." }`: Valid client certificate mandatory.
- **Validation**: Runtime validates `ca_path` exists and is readable.
- **Backend Constraints**: Peer certificate extraction for Rustls mode is currently unimplemented (TODO in code).

---

## Telemetry <a id="telemetry"></a>

### `telemetry.level`
- **Type**: `string` (enum)
- **Required**: Optional
- **Default**: `info`
- **Allowed values**: `error`, `warn`, `info`, `debug`, `trace`.
- **Runtime Effect**: Sets the global logging level.

### `telemetry.pingora`
- **Type**: `string` (enum)
- **Required**: Optional
- **Default**: `info`
- **Allowed values**: `error`, `warn`, `info`, `debug`, `trace`.
- **Runtime Effect**: Sets the logging level for the Pingora framework.

### `telemetry.service_name`
- **Type**: `string`
- **Required**: Optional
- **Default**: `"pavis"`
- **Runtime Effect**: Used in log metadata and tracing spans.

### `telemetry.metrics`
- **Type**: `string`
- **Required**: Optional
- **Alias**: `prometheus_addr`
- **Allowed values**: Valid socket address (IP:PORT).
- **Validation**: Port must be unique across listeners, admin, and metrics.
- **Runtime Effect**: Starts a Prometheus metrics exporter on this address.

### `telemetry.access_log`
- **Type**: `scalar`
- **Required**: Optional
- **Default**: `stdout`
- **Allowed values**: `disabled`, `stdout`, or a file path string.
- **Runtime Effect**: Configures the request logging destination.

### `telemetry.tracing`
- **Type**: `object`
- **Required**: Optional

### `telemetry.tracing.provider`
- **Type**: `string` (enum)
- **Required**: Optional
- **Default**: `otlp`
- **Allowed values**: `otlp`, `jaeger`, `zipkin`.

### `telemetry.tracing.sampling`
- **Type**: `integer`
- **Required**: Optional
- **Default**: `100`
- **Validation**: `0` to `100` (percentage).

### `telemetry.tracing.endpoint`
- **Type**: `string`
- **Required**: Optional
- **Default**: `"http://localhost:4317"`

---

## Upstreams <a id="upstreams"></a>

### `upstreams[].name`
- **Type**: `string`
- **Required**: Yes
- **Validation**: Must be unique across all upstreams. Must not be empty.

### `upstreams[].id`
- **Type**: `integer`
- **Required**: Optional
- **Default**: Sequence starting from 1.
- **Validation**: Must be `> 0`.

### `upstreams[].discovery`
- **Type**: `enum`
- **Required**: Optional
- **Alias**: `discovery_type`
- **Default**: `static`
- **Allowed values**:
  - `static`: Unit variant. IPs provided in `endpoints`.
  - `logical`: Unit variant. Connect-time DNS resolution.
  - `strict: { ttl: <u32> }`: Periodic DNS resolution.

### `upstreams[].balancer`
- **Type**: `string` (enum)
- **Required**: Optional
- **Aliases**: `load_balancer`, `lb`
- **Default**: `random`
- **Allowed values**: `round-robin`, `random`, `least-request`.

### `upstreams[].protocol`
- **Type**: `string` (enum)
- **Required**: Optional
- **Aliases**: `http_version`, `http`
- **Default**: `h1`
- **Allowed values**: `h1` (or `1`, `1.1`, `http1`), `h2` (or `2`, `http2`), `h2h1`.

### `upstreams[].pool`
- **Type**: `object`
- **Required**: Optional
- **Alias**: `connection_pool`

### `upstreams[].pool.idle`
- **Type**: `duration`
- **Required**: Optional
- **Default**: `"60s"`
- **Runtime Effect**: Idle timeout for pooled connections.

### `upstreams[].pool.connect`
- **Type**: `duration`
- **Required**: Optional
- **Default**: `"5s"`
- **Runtime Effect**: TCP connection timeout.

### `upstreams[].pool.max`
- **Type**: `integer`
- **Required**: Required
- **Validation**: Must be `> 0`. No unlimited pools supported (P0 enforcement).
- **Runtime Effect**: Maximum concurrent connections. Enforced with semaphore-based gating.

### `upstreams[].pool.queue`
- **Type**: `object`
- **Required**: Optional
- **Runtime Effect**: Queue configuration for requests when pool is full.

### `upstreams[].pool.queue.capacity`
- **Type**: `integer`
- **Required**: Optional
- **Default**: `0` (no queueing)
- **Runtime Effect**: Maximum number of requests to queue when pool is full.

### `upstreams[].pool.queue.timeout_ms`
- **Type**: `integer`
- **Required**: Optional
- **Default**: `0` (immediate rejection)
- **Runtime Effect**: Maximum time (in milliseconds) to wait in queue.

### `upstreams[].tls`
- **Type**: `object`
- **Required**: Optional

### `upstreams[].tls.enabled`
- **Type**: `boolean`
- **Required**: Optional
- **Default**: `true` (if `tls` object is present).

### `upstreams[].tls.verify_cert`
- **Type**: `boolean`
- **Required**: Optional
- **Default**: `true`

### `upstreams[].tls.verify_hostname`
- **Type**: `boolean`
- **Required**: Optional
- **Default**: `true`

### `upstreams[].tls.sni`
- **Type**: `string`
- **Required**: Optional
- **Validation**: Rejected if `sni_mode` is `auto` or `disabled`.

### `upstreams[].tls.sni_mode`
- **Type**: `string` (enum)
- **Required**: Optional
- **Alias**: `sniMode`
- **Default**: `auto`
- **Allowed values**: `auto`, `name` (requires `sni`), `disabled`.
- **Validation**: `verify=full` (both `verify_cert` and `verify_hostname` are `true`) requires `sni_mode` to be `auto` or `name`.
- **Validation**: `verify=full` with `sni_mode=auto` requires DNS endpoints or a route host rewrite.

### `upstreams[].tls.ca_bundle_path`
- **Type**: `string`
- **Required**: Optional
- **Alias**: `ca_bundle`
- **Validation**: Runtime validates the path exists and is readable when set.
- **Backend Constraint**: **IGNORED** by the Pingora Rustls connector.

### `upstreams[].tls.cert`
- **Type**: `object`
- **Required**: Optional
- **Runtime Effect**: Configures client certificate for mTLS.

### `upstreams[].tls.cert.cert_path`
- **Type**: `string`
- **Required**: Yes
- **Validation**: Runtime validates the path exists and is readable.

### `upstreams[].tls.cert.key_path`
- **Type**: `string`
- **Required**: Yes
- **Validation**: Runtime validates the path exists and is readable.

### `upstreams[].tls.cert.chain_path`
- **Type**: `string`
- **Required**: Optional
- **Validation**: Allowed only if `chain_mode` is `file`.
- **Validation**: Runtime validates the path exists and is readable when set.

### `upstreams[].tls.cert.chain_mode`
- **Type**: `string` (enum)
- **Required**: Optional
- **Default**: `none`
- **Allowed values**: `none`, `embedded`, `file`.

### `upstreams[].endpoints[]`
- **Type**: `list of object`
- **Required**: Yes

### `upstreams[].endpoints[].address`
- **Type**: `string`
- **Required**: Yes
- **Aliases**: `addr`, `ip`

### `upstreams[].endpoints[].port`
- **Type**: `integer`
- **Required**: Yes
- **Validation**: `1` to `65535`.

### `upstreams[].endpoints[].weight`
- **Type**: `integer`
- **Required**: Optional
- **Default**: `1`
- **Validation**: `1` to `65535`.

### `upstreams[].circuit_breaker`
- **Type**: `object`
- **Required**: Optional
- **Runtime Effect**: Enforced (503 on overflow).
- **Fields**:
  - `max_connections` (integer, required)
  - `max_pending_requests` (integer, required)

### `upstreams[].outlier_detection`
- **Type**: `object`
- **Required**: Optional
- **Runtime Effect**: Enforced (ejects endpoints after consecutive failures).
- **Fields**:
  - `consecutive_errors` (integer, required)
  - `eject_duration` (duration, required)

### `upstreams[].health_check`
- **Type**: `object`
- **Required**: Optional
- **Runtime Effect**: Enforced (periodic probes mark endpoints healthy/unhealthy).
- **Fields**:
  - `path` (string, required)
  - `interval` (duration, required)
  - `timeout` (duration, optional; defaults to `interval`)
  - `healthy_threshold` (integer, optional; must be `1`)
  - `unhealthy_threshold` (integer, optional; must be `1`)

---

## Routes <a id="routes"></a>

### `routes[].host`
- **Type**: `string`
- **Required**: Yes
- **Validation**: Must be unique across all virtual hosts.

### `routes[].paths[]`
- **Type**: `list of object`
- **Required**: Yes

### `routes[].paths[].matcher`
- **Type**: `object`
- **Required**: Optional
- **Default**: `prefix: { path: "/" }`

### `routes[].paths[].matcher.path`
- **Type**: `object (tagged enum)`
- **Required**: Yes (if matcher is specified)
- **Allowed Values**:
  - `prefix: { path: "..." }` (YAML tag: `!prefix`)
  - `exact: { path: "..." }` (YAML tag: `!exact`)
  - `regex: { path: "..." }` (YAML tag: `!regex`)
- **Validation**: `path` must be normalized (starts with `/`, no trailing slashes except for `/`) for `prefix` and `exact`. `regex` patterns are limited to 2048 chars.

### `routes[].paths[].matcher.method`
- **Type**: `string`
- **Required**: Optional
- **Allowed Values**: `GET`, `POST`, `PUT`, `DELETE`, `PATCH`, `HEAD`, `OPTIONS`, `CONNECT`, `TRACE` (case-insensitive)
- **Runtime Effect**: Matches requests with this specific HTTP method.

### `routes[].paths[].matcher.methods`
- **Type**: `list of string`
- **Required**: Optional
- **Allowed Values**: List of HTTP methods (same as `method`)
- **Runtime Effect**: Matches requests with any of the listed HTTP methods (OR logic).
- **Validation**: Mutually exclusive with `method` (use one or the other, not both).

### `routes[].paths[].matcher.headers`
- **Type**: `list of object`
- **Required**: Optional
- **Runtime Effect**: All header predicates must match (AND logic).

### `routes[].paths[].matcher.headers[].operator`
- **Type**: `string` (enum)
- **Required**: Optional
- **Default**: `exact`
- **Allowed Values**: `exact`, `prefix`, `regex`, `present`, `absent`

### `routes[].paths[].matcher.headers[].name`
- **Type**: `string`
- **Required**: Yes
- **Validation**: Header name (case-insensitive, alphanumeric + `-` + `_` only).

### `routes[].paths[].matcher.headers[].value`
- **Type**: `string`
- **Required**: Required for `exact` operator
- **Runtime Effect**: Exact value match (case-sensitive).

### `routes[].paths[].matcher.headers[].prefix`
- **Type**: `string`
- **Required**: Required for `prefix` operator
- **Runtime Effect**: Header value must start with this prefix (case-sensitive).

### `routes[].paths[].matcher.headers[].pattern`
- **Type**: `string`
- **Required**: Required for `regex` operator
- **Validation**: Valid regex pattern, max 256 bytes.
- **Runtime Effect**: Header value must match regex pattern. Input limited to 4096 bytes.

### `routes[].paths[].timeout`
- **Type**: `duration`
- **Required**: Optional
- **Default**: `Disabled`
- **Validation**: Must be `> 0` ms and `< u32::MAX` ms.

### `routes[].paths[].retry`
- **Type**: `object`
- **Required**: Optional
- **Runtime Effect**: Enforced during upstream request handling with P2 retry policy.

### `routes[].paths[].retry.max_attempts`
- **Type**: `integer`
- **Required**: Yes
- **Validation**: `1` to `10` (strictly enforced).

### `routes[].paths[].retry.retryable_reasons`
- **Type**: `list of string`
- **Required**: Yes
- **Allowed values**: `status_code`, `connect_timeout`, `read_timeout`, `per_try_timeout`, `pool_full`, `connect_error`.

### `routes[].paths[].retry.retryable_status_codes`
- **Type**: `list of integer`
- **Required**: Required when `status_code` is in `retryable_reasons`
- **Validation**: Cannot be empty when `status_code` retry is enabled.
- **Example**: `[502, 503, 504]`

### `routes[].paths[].retry.backoff`
- **Type**: `object`
- **Required**: Optional
- **Default**: `fixed` with `base_ms: 100`

### `routes[].paths[].retry.backoff.strategy`
- **Type**: `string` (enum)
- **Required**: Optional
- **Default**: `fixed`
- **Allowed values**: `fixed`, `linear`, `exponential`

### `routes[].paths[].retry.backoff.base_ms`
- **Type**: `integer`
- **Required**: Optional
- **Default**: `100`
- **Runtime Effect**: Base delay in milliseconds between retries.

### `routes[].paths[].retry.backoff.max_ms`
- **Type**: `integer`
- **Required**: Required for `exponential` strategy
- **Runtime Effect**: Maximum delay cap for exponential backoff.

### `routes[].paths[].retry.retry_non_idempotent`
- **Type**: `boolean`
- **Required**: Optional
- **Default**: `false`
- **Runtime Effect**: When `true`, allows retrying POST/PATCH/DELETE requests (requires body buffering).

### `routes[].paths[].retry.fail_on_non_replayable_retry`
- **Type**: `boolean`
- **Required**: Optional
- **Default**: `false`
- **Runtime Effect**: When `true`, returns 500 error if retry is needed but request body cannot be buffered.

### `routes[].paths[].retry.max_request_body_buffer_bytes`
- **Type**: `integer`
- **Required**: Optional
- **Default**: `1048576` (1 MB)
- **Runtime Effect**: Maximum request body size to buffer for retry replay.

### `routes[].paths[].retry.per_try`
- **Type**: `duration`
- **Required**: Optional
- **Runtime Effect**: Per-attempt timeout (independent of global request timeout).

### `routes[].paths[].request_headers` / `response_headers`
- **Type**: `object`
- **Required**: Optional
- **Runtime Effect**: Header manipulation rules.

### `routes[].paths[].request_headers.set_headers` / `append_headers` / `add_headers`
- **Type**: `list of [string, string]`
- **Required**: Optional
- **Default**: `[]`

### `routes[].paths[].request_headers.remove_headers`
- **Type**: `list of string`
- **Required**: Optional
- **Default**: `[]`

### `routes[].paths[].rewrite`
- **Type**: `object`
- **Required**: Optional
- **Validation**: **Not supported** with `regex` matcher.

### `routes[].paths[].rewrite.path`
- **Type**: `string`
- **Required**: Optional
- **Runtime Effect**: Replaces the matched path prefix with this value.

### `routes[].paths[].rewrite.host`
- **Type**: `string`
- **Required**: Optional
- **Runtime Effect**: Replaces the `Host` header.

### `routes[].paths[].principal`
- **Type**: `enum`
- **Required**: Optional
- **Default**: `any`
- **Allowed values**:
  - `any`: (unit)
  - `authenticated: { spiffe: "..." }`
  - `prefix: { prefix: "..." }`

### `routes[].paths[].action`
- **Type**: `object (flattened, untagged enum)`
- **Required**: Yes
- **Runtime Effect**: Determines the core operation for the route. The variant is inferred from the presence of specific fields.

#### Variant: Forward
Inferred if `destinations` field is present.
- **destinations[]** (list of object, required):
  - **upstream**: (string, required) Reference to an upstream name.
  - **weight**: (integer, required) Relative weight (`1` to `65535`).

#### Variant: Redirect
Inferred if `status` and `location` fields are present.
- **status** (integer, required): HTTP redirect status code (e.g., `301`, `302`).
- **location** (string, required): Value for the `Location` header.

#### Variant: Direct
Inferred if `status` and `body` fields are present.
- **status** (integer, required): HTTP status code.
- **body** (string, required): Response body.

---

## Cross-Field Rules

1. **Normalized Paths**: All paths in `matcher.path` (except regex) must start with `/` and not end with `/` unless the path is exactly `/`.
2. **Upstream Referencing**: Every `upstream` name in a `forward` action must exist in the top-level `upstreams` list.
3. **mTLS SNI**: `verify=full` with `sni_mode=auto` is rejected unless the upstream uses DNS discovery or the route specifies a `host` rewrite.
4. **Rewrite Conflict**: `rewrite` cannot be used if the `matcher` is `regex`.

---

## Unsupported / Ignored Fields (by Backend)

- **Rustls (Default TLS)**:
  - `upstreams[].tls.ca_bundle_path`: Parsed but ignored by the connector.
  - `listeners[].tls.client_auth`: Peer certificate extraction is currently a TODO.

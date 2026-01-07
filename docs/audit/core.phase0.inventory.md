# Phase 0 — Inventory & API Surface (crates/pavis-core)

## 1. File Inventory

| Relative Path | Classification | Description |
| :--- | :--- | :--- |
| `src/lib.rs` | Glue / Module wiring | Exports runtime types and validation logic. |
| `src/runtime.rs` | Glue / Module wiring | Aggregates runtime modules and defines the root `RuntimeConfig`. |
| `src/serde_impl.rs` | Serialization / Encoding helpers | Custom Serde implementations (e.g., `AccessLogPolicy`). |
| `src/validate.rs` | Validation / Invariants | Defines `CoreValidationError` and the main `validate_runtime` function. |
| `src/runtime/headers.rs` | Canonical types | `HeadersPolicy`, `Headers` struct. |
| `src/runtime/routing.rs` | Canonical types | `VirtualHost`, `Route`, `PathMatch`, `RouteAction`, etc. |
| `src/runtime/server.rs` | Canonical types | `Listener`, `TlsConfig`, `WorkerCount`, etc. |
| `src/runtime/telemetry.rs` | Canonical types | `Telemetry`, `LogLevel`, `Metrics`, `AccessLogPolicy`, `TracingPolicy`. |
| `src/runtime/types.rs` | Canonical types | Newtypes for various configuration values (e.g., `Duration`, `Timeout`, `Hostname`). |
| `src/runtime/upstream.rs` | Canonical types | `Upstream`, `Discovery`, `LoadBalancer`, `Pool`, `Endpoint`, etc. |
| `src/validate/headers.rs` | Validation / Invariants | Validation logic for headers. |
| `src/validate/routes.rs` | Validation / Invariants | Validation logic for routes. |
| `src/validate/server.rs` | Validation / Invariants | Validation logic for server/listener. |
| `src/validate/upstreams.rs` | Validation / Invariants | Validation logic for upstreams. |

## 2. Module Structure

The crate is organized into three main modules:

*   **`runtime`**: Defines the canonical data structures for the configuration.
    *   **`headers`**: Configuration for header manipulation.
    *   **`routing`**: Configuration for virtual hosts, routes, matching, and actions.
    *   **`server`**: Configuration for listeners and TLS.
    *   **`telemetry`**: Configuration for logging, metrics, and tracing.
    *   **`types`**: Common newtypes used across the configuration.
    *   **`upstream`**: Configuration for upstreams, endpoints, load balancing, and connection pooling.
*   **`validate`**: Implements validation logic for the `runtime` types.
    *   **`headers`**: Validates header configurations.
    *   **`routes`**: Validates route configurations (e.g., regex compilation, duplicate routes).
    *   **`server`**: Validates server configurations (e.g., TLS file existence).
    *   **`upstreams`**: Validates upstream configurations (e.g., duplicate names).
*   **`serde_impl`** (private): Contains custom `serde` implementations for specific types.

## 3. Public API Surface

### Structs

*   **`RuntimeConfig`** (`src/runtime.rs`): The root configuration object containing listeners, telemetry, upstreams, and routes.
*   **`ValidatedRuntimeConfig`** (`src/runtime.rs`): A wrapper around `RuntimeConfig` that guarantees the config has passed validation.
*   **`Headers`** (`src/runtime/headers.rs`): Defines rules for setting, appending, adding, and removing headers.
*   **`VirtualHost`** (`src/runtime/routing.rs`): Defines a virtual host and its associated routes.
*   **`Route`** (`src/runtime/routing.rs`): Defines a single route with matching rules, timeouts, retry policy, etc.
*   **`RetryFlags`** (`src/runtime/routing.rs`): A bitmask representing conditions for retrying a request.
*   **`Destination`** (`src/runtime/routing.rs`): Defines a target upstream and weight for traffic forwarding.
*   **`Rewrite`** (`src/runtime/routing.rs`): Defines path and host rewrite rules.
*   **`Listener`** (`src/runtime/server.rs`): Defines a network listener.
*   **`Telemetry`** (`src/runtime/telemetry.rs`): Configuration for system-wide telemetry.
*   **`Duration`** (`src/runtime/types.rs`): A newtype around `NonZeroU32` representing a duration.
*   **`Hostname`** (`src/runtime/types.rs`): A string representing a hostname.
*   **`Host`** (`src/runtime/types.rs`): A string representing a host matcher (e.g., "*.example.com").
*   **`Path`** (`src/runtime/types.rs`): A string representing a URL path.
*   **`ServiceName`** (`src/runtime/types.rs`): A string representing the service name.
*   **`HeaderName`** (`src/runtime/types.rs`): A string representing a HTTP header name.
*   **`HeaderValue`** (`src/runtime/types.rs`): A string representing a HTTP header value.
*   **`UpstreamName`** (`src/runtime/types.rs`): A string representing the name of an upstream.
*   **`UpstreamId`** (`src/runtime/types.rs`): A numeric identifier for an upstream.
*   **`ListenerName`** (`src/runtime/types.rs`): A string representing the name of a listener.
*   **`Port`** (`src/runtime/types.rs`): A numeric representation of a network port.
*   **`Weight`** (`src/runtime/types.rs`): A numeric weight for load balancing.
*   **`SampleRate`** (`src/runtime/types.rs`): A numeric sample rate for tracing.
*   **`Upstream`** (`src/runtime/upstream.rs`): Configuration for an upstream service cluster.
*   **`Pool`** (`src/runtime/upstream.rs`): Configuration for connection pooling.
*   **`Endpoint`** (`src/runtime/upstream.rs`): A single addressable endpoint within an upstream.

### Enums

*   **`CoreValidationError`** (`src/validate.rs`): Enumerates all possible validation errors.
*   **`HeadersPolicy`** (`src/runtime/headers.rs`): Enables or disables header manipulation rules.
*   **`Principal`** (`src/runtime/routing.rs`): Defines the required principal for a route (e.g., authenticated, prefix, any).
*   **`RouteAction`** (`src/runtime/routing.rs`): Defines the action to take for a matched route (Forward, Redirect, Direct).
*   **`RetryPolicy`** (`src/runtime/routing.rs`): Configures retry behavior.
*   **`PathMatch`** (`src/runtime/routing.rs`): Defines how a route matches a path (Prefix, Exact, Regex).
*   **`RewritePath`** (`src/runtime/routing.rs`): Defines how the path should be rewritten.
*   **`RewriteHost`** (`src/runtime/routing.rs`): Defines how the host header should be rewritten.
*   **`WorkerCount`** (`src/runtime/server.rs`): Configures the number of worker threads.
*   **`TlsConfig`** (`src/runtime/server.rs`): Configures TLS for a listener.
*   **`ClientAuth`** (`src/runtime/server.rs`): Configures client certificate authentication requirements.
*   **`LogLevel`** (`src/runtime/telemetry.rs`): Defines logging levels.
*   **`AccessLogPolicy`** (`src/runtime/telemetry.rs`): Configures access logging.
*   **`TracingPolicy`** (`src/runtime/telemetry.rs`): Configures distributed tracing.
*   **`TracingProvider`** (`src/runtime/telemetry.rs`): Specifies the tracing provider (OTLP, Jaeger, Zipkin).
*   **`Metrics`** (`src/runtime/telemetry.rs`): Configures metrics exposure.
*   **`Timeout`** (`src/runtime/types.rs`): Configures a general timeout.
*   **`ConnectTimeout`** (`src/runtime/types.rs`): Configures a connection timeout.
*   **`IdleTimeout`** (`src/runtime/types.rs`): Configures an idle timeout.
*   **`TryTimeout`** (`src/runtime/types.rs`): Configures a timeout per retry attempt.
*   **`Discovery`** (`src/runtime/upstream.rs`): Configures service discovery mechanism.
*   **`LoadBalancer`** (`src/runtime/upstream.rs`): Configures load balancing algorithm.
*   **`HttpVersion`** (`src/runtime/upstream.rs`): Configures the HTTP protocol version.
*   **`ConnectionLimit`** (`src/runtime/upstream.rs`): Configures the maximum number of connections.
*   **`TlsPolicy`** (`src/runtime/upstream.rs`): Configures TLS for upstream connections.
*   **`ClientCert`** (`src/runtime/upstream.rs`): Configures client certificate for upstream connections.
*   **`TlsVerify`** (`src/runtime/upstream.rs`): Configures TLS verification mode.
*   **`SniName`** (`src/runtime/upstream.rs`): Configures SNI for upstream connections.
*   **`EndpointAddr`** (`src/runtime/upstream.rs`): Defines an endpoint address (IP or DNS).

### Functions

*   **`validate_runtime`** (`src/validate.rs`): Validates a `RuntimeConfig` and returns a `CoreValidationResult<ValidatedRuntimeConfig>`.
*   **`ValidatedRuntimeConfig::new`** (`src/runtime.rs`): (Internal/Crate) Creates a new `ValidatedRuntimeConfig`.
*   **`ValidatedRuntimeConfig::assume_validated`** (`src/runtime.rs`): Constructs a `ValidatedRuntimeConfig` assuming prior validation.
*   **`ValidatedRuntimeConfig::from_trusted`** (`src/runtime.rs`): (Unsafe) Constructs a `ValidatedRuntimeConfig` from a trusted source.
*   **`ValidatedRuntimeConfig::into_inner`** (`src/runtime.rs`): Consumes the `ValidatedRuntimeConfig` and returns the inner `RuntimeConfig`.

### Type Aliases

*   **`CoreValidationResult<T>`** (`src/validate.rs`): A result type for core validation operations.

### Constants

*   **`RETRY_FIVE_XX`** (`src/runtime/routing.rs`): Flag for retrying on 5xx errors.
*   **`RETRY_CONNECT_FAILURE`** (`src/runtime/routing.rs`): Flag for retrying on connection failure.
*   **`RETRY_RESET`** (`src/runtime/routing.rs`): Flag for retrying on connection reset.
*   **`RETRY_REFUSED`** (`src/runtime/routing.rs`): Flag for retrying on connection refused.
*   **`RETRY_RESERVED`** (`src/runtime/routing.rs`): Reserved flag bits.

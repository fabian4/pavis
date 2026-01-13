# Pavis Runtime

## 1. Crate Overview
`pavis` is the runtime implementation of the proxy system. It serves as the execution engine that interprets validated configuration and handles network traffic. It is built on top of the `pingora` framework and acts as the concrete application layer that orchestrates routing, upstream management, and telemetry.

Its primary responsibilities are:
- Loading and applying configuration from trusted `.pvs` binary artifacts.
- Managing the lifecycle of the proxy server and its listeners.
- Orchestrating request routing via a deterministic matching engine.
- Managing upstream clusters and load balancing.
- Providing atomic hot-reloading capabilities for configuration updates.
- integrating with a remote configuration relay via the `agent` module.

It explicitly does not handle:
- Configuration validation or compilation (delegated to `pavis-core` and external tools).
- Business logic or complex payload transformation within the proxy layer.
- Protocol implementation details (delegated to `pingora`).

## 2. Features
- **Atomic Hot-Reloading**: Uses `arc_swap` to perform lock-free, atomic replacement of the entire runtime state (router and upstream manager) without dropping active connections.
- **Optimized Routing Engine**: Implements a hybrid matching strategy using hash maps for consecutive exact matches (`ExactMap`) and linear scanning for regex/prefix matches (`Linear`), ensuring deterministic precedence.
- **Validated Artifact Loading**: Restricts configuration loading to `.pvs` files, enforcing a strict separation between configuration generation (validation) and execution.
- **Dynamic Upstream Management**: Manages clusters of upstream endpoints with support for static discovery and load balancing strategies.
- **Telemetry Integration**: Configurable access logging (stdout, file, or disabled) and tracing integration via `tracing-subscriber`.
- **Remote Configuration Agent**: Optional background worker that polls a relay service for configuration updates using a backoff strategy.

## 3. Module Breakdown

### `agent`
Manages the retrieval of dynamic configuration updates. It includes the `ConfigAgent` worker which polls a remote relay URL, handles version negotiation (`lkg_version`), and applies updates to the runtime state using a backoff retry mechanism.

### `load`
Responsible for loading configuration from the filesystem. It enforces the use of `.pvs` extension and deserializes binary artifacts using `pavis-pvs`. It converts the loaded configuration into a `ValidatedRuntimeConfig` under the assumption that artifacts are pre-validated.

### `proxy`
The coordination layer that bridges `pingora`'s proxy traits with the Pavis internal components. It enforces architectural invariants such as non-blocking execution and immutability of state during request handling. It handles header manipulations (`header_ops`) and service definition.

### `router`
Implements the request matching logic. It compiles abstract routes into an executable structure (`CompiledRoute`, `CompiledVirtualHost`). It supports `Exact`, `Prefix`, and `Regex` matching, organizing them into `RouteZone`s to optimize performance while maintaining strict definition order.

### `state`
Manages the global runtime state. It defines `RuntimeState` (holding the `Router` and `UpstreamManager`) and `RuntimeStateHandle`, which provides thread-safe, atomic access to the current configuration via `ArcSwap`.

### `telemetry`
Handles observability signals through three pillars: metrics, access logs, and distributed tracing.

**Components**:
- `access_log`: Non-blocking structured access logging (stdout, file, or disabled)
- `metrics`: Prometheus metrics server with cardinality controls
- `tracing`: OpenTelemetry distributed tracing with OTLP export

**Architectural Principles**:
- Non-blocking operations: All telemetry uses `try_send` or background tasks
- Zero-cost when disabled: Minimal overhead through explicit gating at call sites
- Unified context: `RouterContext` serves as the single observability context per request
- Bounded cardinality: Metrics use route patterns, never raw paths

**Metrics**:
Prometheus metrics are exposed via a dedicated HTTP server configured through `telemetry.metrics.addr`. Metrics include:
- Request metrics (total, duration histograms) with labels: method, route_pattern, status, upstream
- Connection metrics (active gauge, total counter)
- Upstream metrics (requests, duration) with labels: upstream, status

**Access Logging**:
Structured logs emitted per request with timing, routing, and identity metadata. Includes unique request IDs for correlation across systems. Logs are buffered and written asynchronously via a dedicated worker.

**Distributed Tracing**:
OpenTelemetry spans are created for each request with HTTP semantic conventions. Spans include route patterns, upstream selections, RBAC decisions, and final HTTP status. Traces are exported via OTLP to collectors like Jaeger.

### `upstream`
Manages backend clusters and endpoint resolution. It includes the `Manager` which holds `Cluster` instances, and the `UpstreamResolver` service. It handles load balancing and connection pooling configurations for defined upstreams.

## 4. Public API Surface

### `RuntimeStateHandle`
A thread-safe handle allows concurrent access to the current `RuntimeState`.
- `load()`: Returns an `Arc<RuntimeState>` for the current configuration snapshot.
- `store(state: RuntimeState)`: Atomically updates the global state.

### `Router`
The core matching engine.
- `new(routes: Vec<VirtualHost>) -> Result<Self>`: Compiles a list of virtual hosts into an efficient matching structure. Fails if regex compilation fails.
- `match_request(...)`: Resolves an incoming request to a specific `VirtualHost` and `Route`.

### `load_file`
- `load_file(path: &str) -> LoadResult<ValidatedRuntimeConfig>`: The entry point for reading configuration. Enforces the `.pvs` file extension.

### `main` (Binary Entry)
The executable entry point which:
1. Parses CLI arguments (`--config`, `--relay-url`).
2. Initializes the `pingora` server.
3. Sets up listeners based on the loaded configuration.
4. Spawns background services (access logs, upstream resolver, config agent).

## 5. Configuration and Runtime Behavior

### CLI Arguments
- `--config <PATH>`: Mandatory path to a `.pvs` configuration file.
- `--relay-url <URL>`: Optional URL for a remote configuration relay. If provided, the `agent` module is activated.

### Environment Variables
- `RUST_LOG`: Configures the logging level via `tracing_subscriber` (e.g., `info`, `debug`). The runtime automatically maps configuration log levels to these filters.

### Runtime Invariants
- **Immutable Config**: Once loaded, a `RuntimeConfig` is treated as immutable. Changes require a full state swap.
- **Listener Setup**: Listeners are bound at startup based on the initial configuration.

## 6. Error Handling and Invariants

### Error Types
- `RuntimeLoadError`: Encapsulates errors occurring during configuration loading, specifically wrapping `pavis_pvs::PvsError`.
- `anyhow::Result`: Used extensively for internal operations where specific error recovery is not intended (fail-fast initialization).

### Safety Invariants
- **Regex Compilation**: All regular expressions are compiled at initialization (`Router::new`). If a regex is invalid, the runtime refuses to start or update.
- **Panic Safety**: Telemetry modules are designed to drop data rather than panic under load.
- **Lock-Free Hot Path**: The request path avoids `std::sync::Mutex`, relying on `Arc` and `ArcSwap` for state access.

## 7. Non-Goals and Explicit Limitations
- **Mutable Global State**: The runtime does not support mutable global state accessible from request handlers, except through the atomic replacement mechanism.
- **Arbitrary Config Formats**: The runtime deliberately refuses to load YAML, JSON, or other text formats directly; it requires pre-compiled `.pvs` binaries.
- **Blocking I/O**: File system access (other than initial config load) and synchronous network calls are strictly prohibited in the request path.

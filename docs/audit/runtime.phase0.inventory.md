# Audit Phase 0: Inventory & Runtime Surface
- Target: `crates/pavis`
- Timestamp: 2026-01-07T00:00:00Z
- AI Model: gemini-2.0-flash-exp

## 1. File Inventory & Classification

| File Path | Responsibility | Description |
|-----------|----------------|-------------|
| `Cargo.toml` | Config | Dependencies and crate metadata. |
| `src/main.rs` | Entry Point | CLI parsing, config loading, component wiring, server startup. |
| `src/lib.rs` | Module Root | Module exports. |
| `src/load.rs` | Config Loading | Loading and verifying `.pvs` artifacts. |
| `src/state.rs` | State Mgmt | Holds global immutable runtime state (`Router`, `UpstreamManager`). |
| `src/proxy.rs` | Proxy Glue | `Proxy` struct definition and module re-exports. |
| `src/proxy/service.rs` | Proxy Logic | `ProxyHttp` trait impl (request filtering, upstream selection). |
| `src/proxy/context.rs` | Proxy State | Per-request context (`RouterContext`). |
| `src/proxy/header_ops.rs` | Header Logic | Request/Response header manipulation. |
| `src/proxy/identity.rs` | Identity | Identity extraction (stub/placeholder). |
| `src/proxy/service/service_tests.rs` | Tests | Unit tests for proxy service logic. |
| `src/router.rs` | Routing | `Router` struct for matching requests to virtual hosts/routes. |
| `src/router/matcher.rs` | Matching | Path matching logic (Prefix, Exact, Regex). |
| `src/upstream.rs` | Upstream | `UpstreamManager` for cluster/endpoint management. |
| `src/upstream/cluster.rs` | Upstream | `Cluster` struct managing endpoints for a specific upstream. |
| `src/upstream/load_balance.rs` | Upstream | Load balancing logic (Round-robin selection). |
| `src/upstream/resolver.rs` | Upstream | Background DNS resolution (placeholder/skeleton). |
| `src/telemetry.rs` | Telemetry | Telemetry system initialization. |
| `src/telemetry/access_log.rs` | Logging | Access log formatting and writing. |
| `src/agent.rs` | Agent | Config agent module exports. |
| `src/agent/backoff.rs` | Agent | Backoff logic for retries. |
| `src/agent/lkg.rs` | Agent | Last Known Good (LKG) config management. |
| `src/agent/worker.rs` | Agent | Background worker for config updates. |
| `src/agent/worker/agent.rs` | Agent | Agent implementation details. |
| `src/agent/worker/tests.rs` | Tests | Unit tests for agent worker. |

## 2. Module Structure & Architecture

The runtime architecture is designed around the **Immutable Configuration Snapshot** pattern.

### Core Components
1.  **Entry Point (`main.rs`)**:
    -   Parses CLI arguments.
    -   Loads initial configuration via `load::load_file`.
    -   Initializes `RuntimeStateHandle` (wrapping `RuntimeState`).
    -   Configures `pingora::Server` and registers services (`Proxy`, `UpstreamResolver`, `ConfigAgent`).

2.  **State Management (`state.rs`)**:
    -   `RuntimeState`: A read-only struct containing the fully built `Router` and `UpstreamManager`.
    -   `RuntimeStateHandle`: Uses `arc_swap::ArcSwap` to allow atomic hot-swapping of `RuntimeState` at runtime.

3.  **Proxy Engine (`proxy/`)**:
    -   `Proxy`: Implements `pingora::ProxyHttp`. It holds a reference to `RuntimeStateHandle`.
    -   **Request Flow**:
        1.  `new_ctx`: Initializes per-request `RouterContext`.
        2.  `request_filter`: Loads current `RuntimeState`. Calls `router.match_request`. Applies rewrites/headers. Executes `Direct` or `Redirect` actions immediately.
        3.  `upstream_peer`: If action is `Forward`, looks up upstream in `UpstreamManager` (O(1)). Selects endpoint. Configures TLS/HTTP settings.
        4.  `logging`: Writes access logs.

4.  **Routing (`router/`)**:
    -   `Router`: Indexed structure for fast VirtualHost matching (Host header) and Route matching (Path).
    -   `matcher.rs`: Implements specific matching strategies.

5.  **Upstream Management (`upstream/`)**:
    -   `UpstreamManager`: Hash map of upstream names to `Cluster`s.
    -   `Cluster`: Holds `ValidatedUpstream` config and a list of `Endpoint`s.
    -   `UpstreamResolver`: Background service intended to refresh DNS endpoints (currently skeletal).

6.  **Config Agent (`agent/`)**:
    -   Polls a remote relay or control plane.
    -   Upon update, constructs a NEW `RuntimeState` and atomically swaps it into `RuntimeStateHandle`.

## 3. Public Surface

### Entry Points
-   `fn main() -> Result<()>`: The application entry point.

### Public Types (via `lib.rs` exports)
-   **Modules**: `agent`, `load`, `proxy`, `router`, `state`, `telemetry`, `upstream`.

#### `load`
-   `fn load_file(path: &str) -> LoadResult<ValidatedRuntimeConfig>`
-   `enum RuntimeLoadError`

#### `proxy`
-   `struct Proxy`: Main struct used by `main.rs` to register the service.
-   `trait IdentityExtractor`

#### `state`
-   `struct RuntimeState`: The immutable state snapshot.
-   `struct RuntimeStateHandle`: The handle for atomic access/updates.

#### `upstream`
-   `struct UpstreamResolver`: Background service type.

#### `agent`
-   `struct ConfigAgent`: Manages config updates.
-   `struct Backoff`: Utility struct.
-   `fn lkg_version(...)`: Utility function.

#### `telemetry`
-   `struct Telemetry`: Telemetry context.
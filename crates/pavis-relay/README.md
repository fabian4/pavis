# Pavis Relay

## 1. Crate Overview
`pavis-relay` is the central configuration distribution hub for the Pavis ecosystem. It implements the control plane logic, responsible for ingesting, validating, and serving configuration updates to a fleet of Pavis proxy instances.

Its primary responsibilities are:
- Orchestrating automated configuration pipelines (`Ingest -> Codec -> State`).
- Serving a long-polling HTTP API for configuration distribution.
- Managing a versioned history of configuration artifacts (`.pvs`).
- Enforcing policy constraints such as version monotonicity and artifact size limits.
- Providing persistence for the "Last Known Good" (LKG) configuration.

It explicitly does not handle:
- Data plane traffic (delegated to `pavis`).
- Manual configuration editing (delegated to `pavctl`).

## 2. Features
- **Automated Pipelines**: Supports background workers that watch files or remote sources, automatically compiling them into binaries and publishing updates.
- **Long-Polling API**: Implements efficient configuration delivery using `wait_ms` queries, minimizing network overhead and ensuring near-instant updates.
- **Lock-Free State**: Uses `Arc` and `RwLock` for concurrent serving of configuration snapshots while updates are processed in the background.
- **Policy Enforcement**: Prevents configuration rollbacks (version monotonicity) and protects against oversized artifacts.
- **Hot-Reloadable Sources**: Pipelines can be restarted or updated without stopping the relay server.
- **LKG Persistence**: Automatically saves the most recent valid configuration to disk with atomic renames and retry logic.

## 3. Module Breakdown

### `app`
The main entry point for the Axum server. It sets up routing, shared state, and spawns the configured pipelines.

### `pipeline`
Implements the background loop for configuration ingestion. It manages the lifecycle of `Ingest` streams and `Codec` materialization, including exponential backoff for restarts and publish retries.

### `state`
Manages the in-memory cache of configuration versions and history. It handles the atomic transitions between versions and coordinates with the persistence layer.

### `handlers`
Contains the HTTP endpoint implementations:
- `GET /config`: Delivers the current PVS artifact with long-polling support.
- `POST /publish`: Allows manual submission of a signed PVS artifact.
- `GET /artifact/{version}`: Retrieves a specific historical version.
- `GET /status` / `GET /metrics`: Observability and health monitoring.

### `config`
Defines the `RelayConfig` schema (typically `relay.yaml`), specifying pipelines, server options, and persistence settings.

## 4. Public API Surface (HTTP)

### `GET /v1/config`
Headers: `x-pavis-version: <current_version>`
Query: `?wait_ms=5000`
Response: `200 OK` with `.pvs` binary, or `304 Not Modified`.

### `POST /v1/publish`
Headers: `x-pavis-version: <new_version>`
Body: Signed `.pvs` binary.
Response: `200 OK` on successful validation and distribution.

### `GET /v1/metrics`
Exposes Prometheus-formatted metrics (version, publish counts, long-poll activity).

## 5. Configuration and Runtime Behavior

### Configuration (`relay.yaml`)
Relay behavior is highly configurable:
- **Pipelines**: Defines list of sources (files) and their associated codecs.
- **Server**: Port, host, and identification settings.
- **Persistence**: Directory for storing LKG binaries and retry policies for disk I/O.

### Invariants
- **Monotonicity**: Versions must strictly increase.
- **Single Source**: If multiple pipelines target the same relay, the relay enforces version ordering across all updates.
- **Integrity**: Only artifacts that pass `pavis-pvs` verification are accepted for publication.

## 6. Error Handling and Invariants

### Error Model
- `RelayError`: Categorizes failures into `VersionMonotonicity`, `Policy`, `Storage`, etc.
- **Fault Tolerance**: Persistence failures are retried with exponential backoff to handle transient I/O issues.

### Safety
- **Memory Mapping**: Uses `Bytes` for zero-copy sharing of artifacts between the cache and HTTP responses.
- **Atomic Renames**: Persistence uses temporary files and `rename` to ensure the LKG file is never in a partially written state.

## 7. Non-Goals and Explicit Limitations
- **Multi-Tenant Isolation**: Currently designed for a single authoritative configuration set per relay instance.
- **Direct UI**: Does not provide a web-based management interface (API only).
- **Authentication**: Authentication is expected to be handled by a reverse proxy or sidecar in the current implementation.
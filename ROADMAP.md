# Roadmap

> **Reference:** [Architecture.md](./Architecture.md) for technical details on components and protocols.

## Strategic Focus (Iron Triangle)

The roadmap centers on a three-part “iron triangle” determining system viability.

**A. Close the Loop – Dynamic Configuration**
- **Goal**: Enable live, hitless configuration updates in running `pavis` instances.
- **Status**: Complete end-to-end flow: Ingest -> Codec -> Long-Poll -> Hot Reload. Remaining: multi-source failover and observability.

**B. Enable Zero-Copy (mmap-based loading)**
---


---

## Phase 3: Long Polling (Iron Triangle) 🚧

**Goal:** Dynamic configuration updates via HTTP long polling.

**`pavis-relay`** (Control Plane)
- [x] **API & Protocol**
  - [x] **Endpoints**: Implement `GET /v1/config` (long-poll), `GET /v1/status`, `POST /v1/publish`, and `GET /v1/artifacts/{version}`.
  - [x] **Long Polling**: Hold connections (default 60s) until update, handle `X-Pavis-Version` mismatch, and support concurrent clients.
  - [x] **Headers**: Serve `X-Pavis-Version`, `X-Pavis-Checksum`, and `X-Pavis-Checksum-Alg` for integrity.
  - [ ] **Traceability**: Add `X-Pavis-Generated-At` header.
- [x] **State & Storage**
  - [x] **In-Memory Cache**: fast access to current config and history.
  - [x] **Pipeline Ingestion**: Support for `pavis-ingest-file` with debounced watching and automatic versioning.
  - [x] **Durability**: Atomic Last-Known-Good (LKG) persistence with `fsync` safety.
- [x] **Configuration Surface Coverage**
  - [x] **Identity & Bindings**: `identity.*`, `http.admin_bind`, `metrics.prometheus_bind`.
  - [x] **Artifacts**: Naming, paths, limits (`max_pvs_bytes`, `max_routes`), storage backend.
  - [x] **Pipeline**: Source ID, ingest settings, codec selection, and strictness options.
  - [x] **Execution**: Versioning scheme, atomic write/fsync durability.
  - [x] **Distribution**: Long-poll tuning (headers, timeouts) and direct fetch toggles.
  - [x] **Security & Logs**: Auth tokens/modes, logging levels, access log settings.

**`pavctl`**

**`pavis`** (Data Plane)
- [x] **Polling & Updates**
  - [x] **Background Thread**: Periodic polling with exponential backoff and jitter.
  - [x] **Hot Reload**: Atomic `ArcSwap` of runtime config with pre-swap validation and rollback.
  - [ ] **Tuning**: Configurable poll intervals, timeouts, and multi-source failover.
  - [ ] **Visibility**: Config diff logging on update.
- [ ] **Resilience**
  - [x] **Persistence**: Save last-known-good config to disk (`/etc/pavis/config.pvs`).
  - [x] **Recovery**: Boot from disk if control plane is unreachable.
  - [ ] **Safety**: Track last successful reload timestamp for heuristic checks.
- [ ] **Observability**
  - [ ] **Metrics**: Track config version, reload counts (success/fail), and payload size.

**E2E Tests**
- [ ] Config update triggers route change
- [ ] Long poll holds connection until update
- [ ] Checksum mismatch triggers retry
- [ ] Proxy continues serving during config reload
- [ ] Crash recovery loads config from disk
- [ ] Multiple proxies receive same update
- [ ] Exponential backoff on xDS server failure

---

## Architecture Alignment Checklist

- [x] Runtime (`pavis`) depends only on `pavis-core` and `pavis-pvs`.
- [x] `pavis-pvs` performs binary integrity checks only (no semantic validation).
- [x] Codecs call `pavis-core::validate_runtime` after adaptation.
- [ ] Relay (and later Governor) owns migration and re-emits current-version PVS.
- [ ] Compatibility fixtures (vN, vN-1) validated in CI for header/version compatibility.

---

## Optimization & Stability (Immediate Priority) 🚧

**Goal:** Stabilize performance under high concurrency and reduce error rates.

**P0: Connection Management**
- [ ] **Concurrency Limits**: Enforce per-upstream limits on in-flight requests/connections with backpressure.
- [ ] **Connection Reuse**: Enable upstream keepalive and maintain a reusable connection pool to minimize churn.

**P1: Reliability & Noise Reduction**
- [ ] **Idempotent Retries**: Implement limited retries (single attempt) for idempotent methods (e.g., GET) on transient errors.
- [ ] **Log Throttling**: Rate-limit or aggregate repetitive upstream errors; downgrade expected errors during benchmarks.

**P2: Performance Architecture**
- [ ] **Zero-Copy Access**: Refactor runtime to use `ArchivedRuntimeConfig` (mmap) via `rkyv`, removing eager deserialization.
- [ ] **Lazy Compilation**: Implement lazy regex compilation for archived routes.

---

## Planned Enhancements: Core Expansion 🚀

These features address current architectural constraints and are prioritized for the next major development cycle.

### 1. Multi-Listener Support
*   **Goal**: Support multiple bind addresses and protocols (e.g., separate HTTP and HTTPS ports, Admin interfaces) in a single runtime.
*   **Status**: Planned (Currently Single Listener).
*   **Architectural Considerations**:
    *   Refactor `pavis::Proxy` to a Supervisor pattern that spawns and supervises multiple `Pingora` service instances.
    *   Update `ServerConfig` schema to support a list of listeners.
    *   **Extensibility**: Allow listeners to be added/removed dynamically via config reload.

### 2. DNS-Based Upstreams (`LOGICAL_DNS`)
*   **Goal**: Allow routing to dynamic backends defined by hostname, not just static IPs.
*   **Status**: Planned (Currently IP-Only).
*   **Architectural Considerations**:
    *   Integrate an asynchronous DNS resolver (e.g., `trust-dns` or Pingora's resolver) within `UpstreamManager`.
    *   Implement background refresh loops respecting TTL.
    *   Ensure non-blocking resolution during request path.

### 3. Advanced Route Actions
*   **Goal**: Support `DirectResponse`, `Redirect`, `HostRewrite`, and `PathRewrite`.
*   **Status**: Planned (Currently Forwarding Only).
*   **Architectural Considerations**:
    *   **Extensibility**: Introduce a "Route Action" trait or enum in `pavis-core` to decouple action logic from matching logic.
    *   Consider a lightweight plugin system (WASM or Native) for complex transformations to avoid bloating the core.

### 4. TLS Enhancements
*   **Goal**: Support inline certificates (SDS-style) and multiple certs per listener (SNI).
*   **Status**: Planned (Currently File-Path Only).
*   **Architectural Considerations**:
    *   Secure memory handling for inline secrets (zeroing memory).
    *   Integration with Secret Discovery Service (SDS) in the Ingest layer.

---

## Testing & Validation Strategy 🛡️

To ensure smooth expansion and stability as new features are added, the following testing strategy is enforced:

### 1. Unit Testing (Granular)
*   **Scope**: Individual modules (`router`, `header_ops`, `upstream`).
*   **Requirement**: 100% coverage of logic branches for new features (e.g., DNS resolution logic, Rewrite rules).
*   **Tooling**: Standard `cargo test`.

### 2. Integration Testing (Component)
*   **Scope**: Interaction between Core and Runtime components (e.g., Config reload -> Listener update).
*   **Requirement**: Verify that `RuntimeConfig` changes correctly propagate to internal state without restarts.
*   **Mocking**: Use mock DNS resolvers and Upstreams to simulate network variability.

### 3. End-to-End (E2E) Testing (System)
*   **Scope**: Full `pavis` binary against real network targets (Docker/Kind).
*   **Scenarios**:
    *   **Multi-Listener**: Verify traffic on disparate ports.
    *   **DNS**: Verify traffic routing to dynamic IPs (using `dnsmasq` in CI).
    *   **Resilience**: Test behavior when DNS resolution fails or certificates are invalid.
*   **Tooling**: `pavis-e2e` crate (Rust-based test harness).

---

## Historical Phases (Context)

### Phase 1: Foundation ✅

**Goal:** Functional HTTP proxy with static configuration.

**Implementation**
- [x] Cloudflare Pingora integration (`ProxyHttp` trait).
- [x] **Upstreams**: Static IP-based selection (DNS unsupported).
- [x] **Routing**: Basic routing (prefix, exact, regex, wildcard); Forwarding only (no Redirect/Rewrite).
- [x] **Traffic**: Weighted splitting and Round-robin load balancing.
- [x] **Headers**: Request/Response manipulation (Insert/Overwrite behavior).
- [x] **Listener**: Single-listener support with file-based TLS.
- [x] CLI (`--config`) and Docker support.

**E2E Tests**
- [x] Validated basic forwarding, routing logic, traffic weighting, and header ops.

### Phase 2: Protocol 🚧

**Goal:** Define `.pvs` binary format and build tooling.

**Core & Protocol (`pavis-core`, `pavis-pvs`)**
- [x] `RuntimeConfig` schema with `rkyv` derivation.
- [x] `PvsHeader` with Magic Bytes (`PAVS`) and checksum verification.
- [x] Binary integrity validation (no semantic checks in protocol layer).

**Tooling (`pavctl`)**
- [x] `gen`: Compile YAML to `.pvs` with validation.
- [x] `view`: Inspect/debug binary files.
- [x] `check`: Validate YAML without compiling.
- [x] `convert`: Reverse `.pvs` to YAML.

**Runtime (`pavis`)**
- [x] Load `rkyv`-based binary format with version checks.
- [x] Graceful rejection of invalid/mismatched configs.

---

## Deferred Phases (Paused)

### Phase 4: Modular Ingestion ⏸️
*Intentionally deprioritized.*
- Control-plane pipeline migration (accept N-1, emit N).

### Phase 5: Traffic Management ⏸️
*Intentionally deprioritized.*
- Request timeouts, retry policies, circuit breakers.

---

## Future Phases ⏳

### Phase 6: Security
**Goal:** Secure service-to-service communication.
- **mTLS**: Client/Server TLS, certificate management, SPIFFE/SPIRE.
- **Authorization**: RBAC policies, deny-by-default, audit logging.
- **Identity**: JWT validation, JWKS caching.

### Phase 7: Observability
**Goal:** Full visibility into proxy behavior.
- **Metrics**: Prometheus endpoint, request/connection/upstream stats.
- **Tracing**: OpenTelemetry integration (OTLP/Jaeger/Zipkin).
- **Access Logs**: Configurable formats (JSON/Text) and destinations.

### Phase 8: Operations
**Goal:** Production-ready operational features.
- **Health Checks**: Active/Passive checks, outlier detection.
- **Lifecycle**: Graceful shutdown, connection draining.
- **Admin API**: Runtime stats, config dumps, log level changes.

### Phase 9: Advanced Features
**Goal:** Extended functionality for complex use cases.
- **Traffic**: Rate limiting (local/distributed), Fault injection.
- **Transforms**: URL rewriting, body transformation.
- **Protocols**: gRPC, WebSocket, HTTP/2 upstream.
- **Extensibility**: WASM plugins.

### Phase 10: Kubernetes Integration
**Goal:** Native Kubernetes deployment.
- **Operator**: CRDs (`PavisConfig`, `PavisGateway`), Controller.
- **Deployment**: Sidecar injection, Helm charts.
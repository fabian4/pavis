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

## Historical Phases (Context)

### Phase 1: Foundation ✅

**Goal:** Functional HTTP proxy with static configuration.

**Implementation**
- [x] Cloudflare Pingora integration (`ProxyHttp` trait)
- [x] Static upstream selection and basic routing (prefix, exact, regex, wildcard).
- [x] Weighted traffic splitting and load balancing (Round-robin).
- [x] Request/Response header manipulation.
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
# Pavis Roadmap

**Summary**
- **Total**: 38/67
- **Core Features**: 33/46
- **Technical Debt**: 5/21

> **Status**: Active
> **Focus**: Phase 6 (Resilience & Discovery)
> **Reference**: [ARCHITECTURE.md](./ARCHITECTURE.md)

This roadmap distinguishes between **Delivery Phases** (user-visible capabilities) and **Technical Debt** (engineering health and optimization).

## Feature Verification Follow-ups (Code-Based)

### P0 – Safety & Correctness
- [ ] **Header/Method Routing Gap**: Router currently matches path-only despite documentation promising method/header selectors. _Exit criteria_: Router matcher accepts method/header predicates, unit tests cover combos, and E2E proves a method-scoped route is honored.
- [ ] **Route Retries/Timeouts Ignored**: `Route.retry` / `Route.timeout` are parsed but unused. _Exit criteria_: Runtime wires values into Pingora deadlines/retry logic and regression tests exercise success/failure cases.
- [ ] **Upstream `health_check` Dropped**: Codec accepts the field but discards it. _Exit criteria_: Configs either compile to runtime health probes or are rejected with a clear validation error; E2E proves active probe behavior.
- [ ] **Circuit Breaking / `pool.max` Ignored**: Connection limits compile but are unenforced. _Exit criteria_: Runtime enforces `pool.max` and integration tests show capped concurrency.
- [ ] **Inbound mTLS (rustls) Blocked**: Pingora rustls lacks client-cert verifier hooks. _Exit criteria_: Mark config as invalid or gated when rustls backend is selected, plus tests covering rejection. _Blocked on Pingora rustls inbound verifier wiring._
- [ ] **Outbound Custom CA (rustls) Blocked**: Pingora rustls ignores per-peer CA bundles. _Exit criteria_: Either enforce backing logic or reject configs when rustls is active, with tests proving behavior. _Blocked on Pingora rustls per-peer CA support._

### P1 – Process & Test Hardening
- [ ] **Backend-aware E2E Matrix**: Need matrix showing Supported vs Rejected vs Skipped config behaviors. _Exit criteria_: CI publishes the matrix per backend (rustls/OpenSSL) and fails when regressions appear.
- [ ] **Validation Suite for Ignored Fields**: Ensure configs hitting “parsed but ignored / blocked” paths fail fast. _Exit criteria_: New E2E validation suite in the ingest pipeline asserting rejection with precise error messages.

### P2 – Feature Candidates
- [ ] **Header/Method Routing Enhancements**: Extend matcher expressiveness for host+path+method+header logic. _Exit criteria_: Feature flag or GA release with router + codec support plus E2E proving behavior.
- [ ] **Route Retries/Timeouts Implementation**: Full wiring of policy (including per-try budgets). _Exit criteria_: Integration tests demonstrating retry backoff and timeout enforcement.
- [ ] **Active Health / Circuit / Outlier Stack**: Implement probes, breaker enforcement, and passive ejection. _Exit criteria_: Resilience suite covering healthy/unhealthy transitions and breaker trips.

**Architectural Constraint: Frozen Data Plane**
This roadmap is strictly bounded by the Frozen Data Plane architecture. Features that require runtime code generation, interpretation, or non-deterministic policy evaluation (e.g., WASM, Lua, global rate limiting) are **structurally excluded**. All capabilities must be resolvable at compile-time (Codec stage).

---

# A. Delivery Roadmap (Feature-Oriented)

## Phase 1: Foundation (Core Proxy)
> **Goal**: A functional, programmable HTTP proxy based on Pingora.
> **Status**: ✅ Complete

- [x] **Core Engine**: Implementation of `ProxyHttp` trait (Pingora integration).
- [x] **Routing**: Prefix, exact, regex, and wildcard matching (Compiled ahead-of-time).
- [x] **Traffic Management**: Weighted splitting and Round-robin load balancing.
- [x] **Headers**: Request/Response manipulation (Insert/Overwrite).
- [x] **TLS**: Single-listener support with file-based certificates.
- [x] **Upstreams**: Static IP-based selection.
- [x] **CLI**: `pavis` runtime and `pavctl` tooling support.

## Phase 2: Protocol & Tooling (The PVS Format)
> **Goal**: A zero-copy, verifiable binary configuration protocol.
> **Status**: ✅ Complete

- [x] **Schema**: `pavis-core` RuntimeConfig with `rkyv` derivation.
- [x] **Integrity**: Magic Bytes (`PAVS`), versioning, and checksum verification.
- [x] **Tooling**: `pavctl` commands for generation (`gen`), inspection (`view`), check (`check`), and reverse conversion (`convert`).
- [x] **Safety**: Graceful rejection of invalid, corrupt, or version-mismatched binaries.
- [ ] [MUST] **Versioning Policy**: Define strict forward/backward compatibility rules for .pvs artifacts.

## Phase 3: Dynamic Configuration (The Control Loop)
> **Goal**: Live, hitless reconfiguration via atomic artifact swapping.
> **Status**: 🚧 In Progress

- [x] **Pipeline**: Source ingestion (File) -> Codec transformation -> Artifact generation.
- [x] **Distribution API**: ETag-based long-polling endpoints (`GET /v1/config`, `POST /v1/publish`).
- [x] **Hot Reload**: Atomic `ArcSwap` of frozen runtime state without dropping connections.
- [x] **Durability**: Last-Known-Good (LKG) persistence to disk (`/etc/pavis/config.pvs`) with fsync.
- [x] **Recovery**: Boot from LKG if control plane is unreachable.
- [x] **Identity**: Configurable bindings for Admin and Prometheus interfaces.
- [x] **Traceability**: `X-Pavis-Generated-At` headers for lineage tracking.
- [ ] [MUST] **CI/CD Pipeline**: Automated Multi-arch Docker builds (linux/amd64, linux/arm64) and Cargo publishing.

## Phase 3.5: Architecture Convergence & Boundary Hardening
> **Goal**: Enforce strict separation between Codec (Compiler) and Runtime (Executor).
> **Status**: ✅ Complete (Prerequisite for Phase 4)

- [x] **Typed Pipeline Stages**: Explicit, non-bypassable stages (Artifact -> CheckedArtifact -> RuntimeConfig -> ValidatedRuntimeConfig -> PVS).
- [x] **Dependency Inversion**: `pavis-relay` depends on ingest/codec traits, not concrete implementations.
- [x] **Plugin-Style Composition**: Feature-gated ingest/codec modules to keep binaries small and extensible.
- [x] **Boundary Enforcement**: Relay remains an execution/distribution engine; no semantic config interpretation.
- [x] **Convergence Gate**: No Phase 4 expansion until the above are complete and validated.
- [x] [MUST] **Test Harness**: Standardized integration test bed (Relay + Proxy + Mock Backend + Traffic Gen) for regression testing.

## Phase 4: Security & Identity (Critical Path)
> **Goal**: Enterprise-grade security via frozen policies.
> **Status**: ⚠️ Partial (TLS Backend Limitations)

- [x] **TLS Termination**: Server-side TLS with single certificate per listener (No SNI).
- [ ] **Inbound mTLS (Client Cert Validation)**: Blocked on Pingora rustls backend. Available with OpenSSL backend.
- [ ] **Outbound mTLS (Custom CA Verification)**: Blocked on Pingora rustls backend. Available with OpenSSL backend.
- [x] **Authorization (RBAC)**: Static Path/Method based policies (Deny-by-default).
- [x] **Identity**: Integration with SPIRE/SPIFFE workload identities (SPIFFE ID extraction available with OpenSSL backend).

**TLS Backend Note**: The current default build uses Pingora's rustls connector, which does not support inbound client certificate authentication or per-peer CA verification. These features are available when building with the OpenSSL/BoringSSL backend. Pavis is waiting for upstream Pingora to add rustls support for these capabilities.

## Phase 5: Observability (Critical Path)
> **Goal**: Deep visibility into proxy behavior required for Operations.
> **Status**: ✅ Complete

- [x] **Prometheus Metrics**: Exporter with request, connection, and upstream dimensions.
- [x] **Access Logs**: Configurable JSON/Text output to stdout or file.
- [x] **Distributed Tracing**: OpenTelemetry (OTLP) integration for request tracing.
- [x] **Runtime Stats**: Internal telemetry (config version, reload counts, payload size).

## Phase 6: Resilience & Discovery
> **Goal**: Bounded dynamic behavior for reliability.
> **Status**: ⏳ Planned

- [ ] **Outlier Detection**: Passive health checks (eject 5xx pods).
- [ ] **Circuit Breaking**: Connection limits and max pending requests.
- [x] **DNS Discovery**: `StrictDns` (TTL-based) and `LogicalDns` (Lazy) support.
- [ ] **Active Health Checks**: Proactive `/healthz` pings.

## Phase 7: Operational Lifecycle
> **Goal**: Production readiness and ease of operation.
> **Status**: ⏳ Planned

- [ ] **Graceful Shutdown**: Connection draining sequences.
- [ ] **Admin API**: Read-only runtime inspection endpoints (`/admin/health`, `/admin/stats`).

## Phase 8: xDS & Service Mesh Integration
> **Goal**: Compile-time adaptation of external control planes.
> **Status**: ⚠️ Deferred (Blocked by Security & Observability)

- [ ] **xDS Ingest**: gRPC-based ADS (Aggregated Discovery Service) implementation.
- [ ] **xDS Codec**: Map LDS, RDS, CDS, and EDS into `RuntimeConfig` (Compiler pass).
- [ ] **State Synchronization**: Handle snapshot consistency and resource tracking.

## Phase 9: Kubernetes Integration
> **Goal**: Native Kubernetes operator and deployment models.
> **Status**: ⏳ Planned

- [ ] **Operator**: Controller for `PavisConfig` and `PavisGateway` CRDs.
- [ ] **Deployment**: Sidecar injector webhook and Helm charts.

---

# B. Technical Debt Register

> **Definition**: Deferred engineering work, optimizations, and architectural alignment tasks.
> **Policy**: Must be reviewed before starting new Delivery Phases. TD-3 items are mandatory prerequisites (tracked in Phase 3.5).

### TD-1: Testing & Quality Assurance
- [x] **[Safety] Unit Testing Gaps**: Low confidence in edge cases for new features. (Trigger: Before Phase 4)
- [x] **[Safety] Integration Testing**: Risk of state desync during reloads. (Trigger: Before Phase 4)
- [x] **[Safety] E2E Testing**: No validation against real network targets/kernels. (Trigger: Before v1.0 Release)
- [ ] **[Safety] Symlink Verification**: Verify ingest correctly follows symbolic links. (Trigger: Next Release)

### TD-2: Release Engineering & Safety
- [ ] **[Safety] CI Compatibility Fixtures**: Risk of breaking backward compatibility for older binaries. (Trigger: Before first public release)
- [ ] **[Arch] Governance Ownership**: Relay currently owns migration logic; should move to Governor. (Trigger: When Governor component is introduced)
- [x] **[Safety] Strict Format Sniffing**: Verify file content type bytes, not just extension. (Trigger: Phase 4)

### TD-3: Architectural Coupling (Relay)
- [ ] **[Arch] Reclassified to Phase 3.5**: Architecture Convergence & Boundary Hardening is mandatory before Phase 4.
- [ ] **[DX] Binary Size/Compile-Time Polish**: Optimize relay build after feature gating is in place. (Trigger: After Phase 3.5)

### TD-4: Performance Optimizations
- [x] **[Perf] Zero-Copy Loading (mmap)**: Config loading copies bytes to heap; increases startup RAM. (Trigger: Config sizes > 10MB)
- [ ] **[Perf] Lazy Regex Compilation**: Startup penalty for configs with many regex routes. (Trigger: > 100 regex routes)
- [ ] **[Perf] Connection Reuse**: High TCP churn; increased latency. (Trigger: High-throughput stress tests)

### TD-5: Resilience & Scalability
- [ ] **[Scale] Concurrency Limits**: No protection against "thundering herd" or DOS. (Trigger: Public internet exposure)
- [ ] **[Resilience] xDS Backoff**: Aggressive retries can DDOS the control plane. (Trigger: Deployment to large mesh)
- [ ] **[Resilience] Idempotent Retries**: Network blips cause user-visible errors. (Trigger: High availability SLA requirements)

### TD-6: Service Mesh & xDS
- [ ] **[Perf] Delta xDS (Incremental)**: High CPU/Network for large meshes; full snapshot transfers. (Trigger: > 1000 Envoy resources)
- [ ] **[Arch] Stateful Resource Tracking**: Potential for stale endpoints if EDS/CDS desync. (Trigger: Multi-cluster deployments)

### TD-7: Bench TODO
- [ ] **[Bench] System / Kubernetes (kind) Mode**: Implement lifecycle-oriented benchmarks (Configuration Reload, Rollback, Recovery) in a full cluster context. (Planned: Post-Standalone stabilization)
- [ ] **[Bench] Protocol & Payload Coverage**: Add TLS-on/TLS-off variants, HTTP/2/gRPC workloads, and large-payload/streaming cases so production traffic patterns are exercised.
- [ ] **[Bench] Saturation Profiles**: Implement a combined high-concurrency + open-loop saturation case and the memory-limited resource profile promised in the methodology.
- [ ] **[Bench] Metrics & Reporting**: Surface target-vs-achieved RPS deltas, p999 for closed-loop tests, detailed error classes, and network/disk stats, then embed host hardware summaries, Docker image digests, and cpuset verification data directly in the generated reports for auditability.

---

# C. Future Hardening (Deferred / Optional)

> **Definition**: Potential reliability improvements that are explicitly **out of scope** for current development phases. These are not required for functional correctness and represent optional hardening work that may be revisited once core persistence semantics and file layout stabilize.
> **Status**: Deferred indefinitely. Not part of CI or release planning.

## Crash-Consistency Hardening via Failpoints

**Status**: Future / Deferred / Optional Hardening

### Motivation
Deterministic testing of crash windows during configuration publish and apply operations could validate persistence atomicity guarantees:
- **Publish crash windows**: Verify Last-Known-Good (LKG) invariants when crashes occur during relay publish operations (write, fsync, rename).
- **Apply crash windows**: Test runtime startup reconciliation when crashes occur during proxy config application.
- **Invariant validation**: Ensure history log integrity, startup recovery semantics, and LKG fallback behavior under abnormal termination.

### Why Deferred
- **Added complexity**: Failpoint infrastructure (conditional panic injection, test orchestration) increases maintenance burden.
- **Premature optimization**: Requires persistence semantics (fsync ordering, rename atomicity, history log format) and on-disk file layout to be fully stable and frozen.
- **Coverage overlap**: Current integrated and functional E2E tests already validate primary correctness paths (successful publish, successful apply, graceful reload).

### Scope (If Revisited)
- **Relay publish crash windows**: Failpoints at each step of the publish pipeline (pre-write, post-write/pre-fsync, post-fsync/pre-rename, post-rename).
- **Optional runtime apply crash windows**: Failpoints during proxy startup reconciliation and config swap operations.
- **Test infrastructure**: Deterministic crash injection, post-crash recovery validation, and automated invariant checking.

### Explicit Non-Goals
- **Not required for functional correctness**: Crash-consistency testing is a hardening measure, not a prerequisite for release or deployment.
- **Not part of normal CI**: Would run as optional, manually-triggered validation only—not in default builds or PR checks.
- **Not a replacement for E2E tests**: Existing E2E coverage remains the primary validation mechanism for feature correctness and operational behavior.

---

# Appendix: Explicitly Dropped / Out of Scope

The following features have been evaluated and explicitly rejected to maintain Pavis's lightweight and pragmatic design philosophy.

- **Wasm Plugins**: High complexity and runtime overhead.
- **Lua Scripting**: Unpredictable latency variance.
- **gRPC Transcoding**: Better handled by dedicated gateways or generated clients.
- **Global Rate Limiting**: Requires external dependencies (Redis/gRPC) that bloat the sidecar.
- **SNI Multi-Cert**: Sidecars typically manage a single workload identity.
- **External Auth (OIDC/OAuth)**: Pavis handles Service-to-Service auth, not end-user login.
- **WAF (ModSecurity)**: Performance intensive; use a dedicated edge firewall.

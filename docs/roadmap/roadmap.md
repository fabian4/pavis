# Pavis Roadmap

**Summary**
- **Total**: 52/82
- **Core Features**: 41/46
- **Technical Debt**: 9/25

> **Status**: Active
> **Focus**: Phase 7 (Operational Lifecycle)
> **Reference**: [ARCHITECTURE.md](../../ARCHITECTURE.md)

Pavis is a **Frozen Data Plane execution system**. Multiple ingest frontends (File, xDS, Kubernetes, and future adapters) feed a single semantic compiler pipeline (Codec ➝ RuntimeConfig ➝ PVS). The Relay component is a dumb artifact distributor, and the Runtime is a dumb artifact executor. There is **no runtime interpretation, no dynamic policy evaluation, and no runtime code generation**—all semantics are decided before artifacts are sealed.

This roadmap distinguishes between **Delivery Phases** (user-visible capabilities) and **Technical Debt** (engineering health and optimization).

## Feature Verification Follow-ups (Semantic Closure)

_Release blockers_: Phases 4 and 7 cannot be marked complete until every item below is resolved; these items codify the Frozen Data Plane contract.

### P0 – Safety & Correctness
- [x] **Header/Method Routing Gap**: Router matcher now accepts method/header predicates with multiple header support (AND logic). Unit tests cover combinations, E2E tests prove method-scoped and header-scoped routing behavior. Implementation: `pavis/src/router.rs`, tests: `pavis/tests/routing.rs`, `tests/suites/pavis/52_routing_method_header_predicates.sh`.
- [x] **Route Retries/Timeouts Ignored**: `Route.retry` / `Route.timeout` are parsed but unused. _Exit criteria_: Runtime wires values into Pingora deadlines/retry logic and regression tests exercise success/failure cases.
- [x] **Upstream `health_check` Dropped**: Codec accepts the field but discards it. _Exit criteria_: Configs either compile to runtime health probes or are rejected with a clear validation error; E2E proves active probe behavior.
- [x] **Upstream `pool.max` Enforcement**: Connection limits are now enforced with semaphore-based gating. Supports queue capacity and timeout parameters. Integration tests verify capped concurrency and deterministic rejection behavior. Implementation: `pavis/src/upstream/cluster.rs` (lines 85-174), E2E tests: `tests/suites/pavis/80-83_pool_*.sh`.
- [x] **Inbound mTLS**: OpenSSL backend enforces client cert verification; TLS E2E suites cover required/optional behavior.
- [x] **Outbound Custom CA**: OpenSSL backend honors per-upstream CA bundles and client certs.

### P1 – Process & Test Hardening
- [x] **TLS E2E Coverage (OpenSSL)**: TLS/mTLS suites are active and enforced in CI.

### P2 – Feature Candidates
- [x] **Header/Method Routing Enhancements**: Advanced matcher support including multi-method predicates (`methods: ["GET", "POST"]`), header operators (exact/prefix/regex/present/absent), compound AND logic for multiple headers. OR/NOT predicates deferred. Implementation: `crates/pavis-core/src/runtime/routing.rs`, `crates/pavis-codec-serde/src/config/convert/routes.rs`, `crates/pavis/src/router/matcher.rs`, `crates/pavis/src/regex_validator.rs`. Tests: Unit tests in `matcher.rs`, E2E test `tests/suites/pavis/52_routing_method_header_predicates.sh`.
- [x] **Route Retries/Timeouts Implementation**: Full P2 retry policy with backoff strategies (fixed, linear, exponential), retryable reasons filtering, idempotency constraints, and request body buffering. Implementation: `pavis-core/src/runtime/retry.rs`, `pavis-codec-serde/src/config/convert/routes.rs`, `pavis/src/retry.rs`. Tests: `pavis-core/src/runtime/retry.rs` (11 tests), `pavis-codec-serde/tests/retry_policy_tests.rs` (12 tests), `tests/suites/pavis/93-96_retry_*.sh`. Status: Complete.
- [x] **Active Health / Circuit / Outlier Stack**: Implement probes, breaker enforcement, and passive ejection. _Exit criteria_: Resilience suite covering healthy/unhealthy transitions and breaker trips.

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
> **Status**: ⚠️ Incomplete (Versioning policy not frozen)

- [x] **Schema**: `pavis-core` RuntimeConfig with `rkyv` derivation.
- [x] **Integrity**: Magic Bytes (`PAVS`), versioning, and checksum verification.
- [x] **Tooling**: `pavctl` commands for generation (`gen`), inspection (`view`), check (`check`), and reverse conversion (`convert`).
- [x] **Safety**: Graceful rejection of invalid, corrupt, or version-mismatched binaries.
- [ ] [MUST] **Versioning Policy**: Define strict forward/backward compatibility rules for .pvs artifacts. _No public release is permitted until these rules are frozen._

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
> **Status**: ✅ Complete

- [x] **TLS Termination**: Server-side TLS with single certificate per listener (No SNI).
- [x] **Inbound mTLS (Client Cert Validation)**: Enforced via OpenSSL backend.
- [x] **Outbound mTLS (Custom CA Verification)**: Per-upstream CA bundles and client certs supported via OpenSSL backend.
- [x] **Authorization (RBAC)**: Static Path/Method based policies (Deny-by-default).
- [x] **Identity**: Integration with SPIRE/SPIFFE workload identities (SPIFFE ID extraction available with OpenSSL backend).

**TLS Backend Note**: The runtime is OpenSSL-only; rustls builds are not supported or tested in CI.

## Phase 5: Observability (Critical Path)
> **Goal**: Deep visibility into proxy behavior required for Operations.
> **Status**: ✅ Complete

- [x] **Prometheus Metrics**: Exporter with request, connection, and upstream dimensions.
- [x] **Access Logs**: Configurable JSON/Text output to stdout or file.
- [x] **Distributed Tracing**: OpenTelemetry (OTLP) integration for request tracing.
- [x] **Runtime Stats**: Internal telemetry (config version, reload counts, payload size).

## Phase 6: Resilience & Discovery
> **Goal**: Bounded dynamic behavior for reliability.
> **Status**: 🚧 In Progress

- [x] **Outlier Detection**: Passive health checks (eject 5xx pods).
- [x] **Circuit Breaking**: Connection limits and max pending requests.
- [x] **DNS Discovery**: `StrictDns` (TTL-based) and `LogicalDns` (Lazy) support.
- [x] **Active Health Checks**: Proactive `/healthz` pings.

## Phase 7: Operational Lifecycle
> **Goal**: Production readiness and ease of operation.
> **Status**: ✅ Complete

- [x] **Graceful Shutdown**: Connection draining sequences with configurable timeout.
- [x] **Admin API**: Read-only runtime inspection endpoints (`/health`, `/stats`).

## Phase 7.5: TLS Backend Migration
> **Goal**: Standardize on a single, feature-complete TLS backend.
> **Status**: ✅ Complete

- [x] **OpenSSL-only runtime build** with Pingora OpenSSL backend.
- [x] **TLS/mTLS E2E suites enabled** (no rustls skips).

## Phase 8: xDS Ingest & Codec (Envoy Control Plane → Frozen Data Plane)
> **Goal**: Compile-time adaptation of xDS control planes into Frozen Data Plane artifacts.
> **Status**: ⚠️ Deferred (Blocked by prerequisites)

- [ ] **xDS Ingest Adapter**: gRPC ADS client that captures LDS/RDS/CDS/EDS snapshots but never runs inside the runtime.
- [ ] **xDS Codec Pass**: Compile LDS/RDS/CDS/EDS resources into `RuntimeConfig`, then seal into `.pvs` artifacts; runtime never speaks xDS or ADS.
- [ ] **State Synchronization**: Ensure snapshot consistency, deterministic ordering, and artifact publication via Relay.

_Runtime must NOT speak xDS or run ADS; compiled artifacts are the only interface presented to executors._

_Blockers_: PVS versioning policy not frozen (Phase 2) and codec/runtime semantics not frozen (Phase 3.5 guardrails must be revalidated for xDS inputs).

## Phase 9: Kubernetes Ingest & Publishing Pipeline (CRDs → Frozen Data Plane)
> **Goal**: Native Kubernetes operator and deployment models.
> **Status**: ⏳ Planned

- [ ] **Operator**: Controller for `PavisConfig` and `PavisGateway` CRDs; CRDs are compiled into `RuntimeConfig`, then sealed into `.pvs` artifacts.
- [ ] **Deployment**: Sidecar injector webhook and Helm charts; Relay distributes generated artifacts, and the runtime performs zero mutation or dynamic policy logic.

---

# B. Technical Debt Register

> **Definition**: Deferred engineering work, optimizations, and architectural alignment tasks.
> **Policy**: Must be reviewed before starting new Delivery Phases. TD-3 items are mandatory prerequisites (tracked in Phase 3.5).

### TD-1: Testing & Quality Assurance
- [x] **[Safety] Unit Testing Gaps**: Low confidence in edge cases for new features. (Trigger: Before Phase 4)
- [x] **[Safety] Integration Testing**: Risk of state desync during reloads. (Trigger: Before Phase 4)
- [x] **[Safety] E2E Testing**: No validation against real network targets/kernels. (Trigger: Before v1.0 Release)
- [x] **[DX] Context Artifacts for Scripts**: Standardized `context.env` for benchmarks/tests with case-scoped copies. (Trigger: Phase 7)
- [x] **[DX] Docker E2E Upstream Image Default**: Default docker-mode mock upstream image for standalone upstream tests. (Trigger: Phase 7)
- [ ] **[Safety] Symlink Verification**: Verify ingest correctly follows symbolic links. (Trigger: Next Release)

### TD-2: Release Engineering & Safety
- [ ] **[Safety] CI Compatibility Fixtures**: Risk of breaking backward compatibility for older binaries. (Trigger: Before first public release)
- [ ] **[Arch] Governance Ownership**: Relay currently owns migration logic; should move to Governor. (Trigger: When Governor component is introduced)
- [x] **[Safety] Strict Format Sniffing**: Verify file content type bytes, not just extension. (Trigger: Phase 4)

### TD-3: Architectural Coupling (Relay)
_Phase 3.5 is the authoritative gate for compiler/runtime boundary hardening; TD-3 now tracks only post-gate relay polish._
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
- [x] **[Bench] Non-Linux Pinning/Limit Bypass**: Allow benchmarks to run without `taskset` or memory limits on non-Linux hosts; warn when pinning/limits are skipped.
- [x] **[Bench] Standalone Case Defaults**: Fix standalone case defaults for `bench/docker-compose.yaml` and `bench/scripts/pretty.sh`.
- [ ] **[Bench] System / Kubernetes (kind) Mode**: Implement lifecycle-oriented benchmarks (Configuration Reload, Rollback, Recovery) in a full cluster context. (Planned: Post-Standalone stabilization)
- [ ] **[Bench] Protocol & Payload Coverage**: Add TLS-on/TLS-off variants, HTTP/2/gRPC workloads, and large-payload/streaming cases so production traffic patterns are exercised.
- [ ] **[Bench] Saturation Profiles**: Implement a combined high-concurrency + open-loop saturation case and the memory-limited resource profile promised in the methodology.
- [ ] **[Bench] Metrics & Reporting**: Surface target-vs-achieved RPS deltas, p999 for closed-loop tests, detailed error classes, and network/disk stats, then embed host hardware summaries, Docker image digests, and cpuset verification data directly in the generated reports for auditability.
- [ ] **[Bench][Bug] System upstream name DNS collision**: In system benchmarks, upstream named `backend` resolves via cluster DNS to a public domain (`backend.fabianbao.xyz`), causing 502s and preventing version/fingerprint checks. Workaround: system-mode configs now use `local-backend`. Root cause suspected: runtime resolves upstream by name instead of configured endpoint address; needs core fix.

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

## Non-Goals / Out of Scope
- No WASM
- No Lua
- No runtime xDS
- No dynamic policy logic
- No global rate limiting
- No SNI multi-cert
- No external auth (OIDC)
- No WAF

## Phase Gates (Hard)
- **Phase 4** is blocked until Phase 7.5 (Semantic Closure & Backend Parity) is complete.
- **Phase 8** is blocked until the PVS versioning policy is frozen (Phase 2) and codec/runtime semantics are frozen per Phase 3.5 guardrails.
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

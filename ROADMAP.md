# Pavis Roadmap

**Summary**
- **Total**: 19/67
- **Core Features**: 18/50
- **Technical Debt**: 1/17

> **Status**: Active
> **Focus**: Phase 3 (Dynamic Config) & Phase 3.5 (Architecture Convergence)
> **Reference**: [ARCHITECTURE.md](./ARCHITECTURE.md)

This roadmap distinguishes between **Delivery Phases** (user-visible capabilities) and **Technical Debt** (engineering health and optimization).

**Non-Goals (Intentional Scope Limits)**
- No inbound policy engine
- No runtime governance
- No "smart proxy" behavior

**Architectural Rationale (Why Convergence Precedes Expansion)**
- xDS increases configuration surface area; convergence prevents that complexity from leaking into Relay or Runtime.
- Explicit, typed, non-bypassable pipeline stages and strict Runtime/Relay/Codec/Ingest separation keep the relay thin, preserve correctness guarantees, and make external control-plane work safe to scale.

---

# A. Delivery Roadmap (Feature-Oriented)

## Phase 1: Foundation (Core Proxy)
> **Goal**: A functional, programmable HTTP proxy based on Pingora.
> **Status**: ✅ Complete

- [x] **Core Engine**: Implementation of `ProxyHttp` trait (Pingora integration).
- [x] **Routing**: Prefix, exact, regex, and wildcard matching.
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

## Phase 3: Dynamic Configuration (The Control Loop)
> **Goal**: Live, hitless reconfiguration from external sources.
> **Status**: 🚧 In Progress

- [x] **Pipeline**: Source ingestion (File) -> Codec transformation -> Artifact generation.
- [x] **Distribution API**: Long-polling endpoints (`GET /v1/config`, `POST /v1/publish`).
- [x] **Hot Reload**: Atomic `ArcSwap` of runtime state without dropping connections.
- [x] **Durability**: Last-Known-Good (LKG) persistence to disk (`/etc/pavis/config.pvs`) with fsync.
- [x] **Recovery**: Boot from LKG if control plane is unreachable.
- [x] **Identity**: Configurable bindings for Admin and Prometheus interfaces.
- [x] **Traceability**: `X-Pavis-Generated-At` headers for lineage tracking.

## Phase 3.5: Architecture Convergence & Boundary Hardening
> **Goal**: Harden pipeline stages and enforce strict component boundaries before xDS expansion. This introduces no new user-visible features.
> **Status**: ⏳ Planned (Prerequisite for Phase 4)

- [x] **Typed Pipeline Stages**: Explicit, non-bypassable stages (Artifact -> CheckedArtifact -> RuntimeConfig -> ValidatedRuntimeConfig -> PVS).
- [x] **Dependency Inversion**: `pavis-relay` depends on ingest/codec traits, not concrete implementations.
- [x] **Plugin-Style Composition**: Feature-gated ingest/codec modules to keep binaries small and extensible.
- [x] **Boundary Enforcement**: Relay remains an execution/distribution engine; no semantic config interpretation.
- [ ] **Convergence Gate**: No Phase 4 expansion until the above are complete and validated.

## Phase 4: xDS & Service Mesh Integration
> **Goal**: First-class integration with external control planes (Istio, Kuma).
> **Status**: 🚧 In Progress (Codec exploration only; blocked by Phase 3.5)

- [ ] **Prerequisite Gate**: Phase 3.5 complete; TD-1 and TD-2 safety checks reviewed.
- [ ] **Boundary Guardrail**: xDS complexity must not leak into Relay or Runtime.
- [ ] **xDS Ingest**: gRPC-based ADS (Aggregated Discovery Service) implementation.
- [ ] **xDS Codec**: Map LDS, RDS, CDS, and EDS into `RuntimeConfig`.
- [ ] **State Synchronization**: Handle snapshot consistency and resource tracking.
- [ ] **Istio Compatibility**: Verified integration with Istio Discovery (pilot).
- [ ] **Kuma Compatibility**: Verified integration with Kuma Control Plane.

## Phase 5: Security (TLS & Auth)
> **Goal**: Enterprise-grade security capabilities.
> **Status**: ⏳ Planned

- [ ] **Advanced TLS**: SNI support (multiple certs per listener) and SDS-style inline certificates.
- [ ] **mTLS**: Mutual TLS for downstream (client) and upstream (server) connections.
- [ ] **SPIFFE/SPIRE**: Workload identity integration.
- [ ] **Authorization**: RBAC policies (Deny-by-default) and audit logging.
- [ ] **Identity**: JWT validation and JWKS caching.

## Phase 6: Observability
> **Goal**: Deep visibility into proxy behavior.
> **Status**: ⏳ Planned

- [ ] **Metrics**: Prometheus exporter with request, connection, and upstream dimensions.
- [ ] **Access Logs**: Configurable JSON/Text output to stdout or file.
- [ ] **Tracing**: OpenTelemetry (OTLP) integration for distributed request tracing.
- [ ] **Runtime Stats**: Internal telemetry (config version, reload counts, payload size).

## Phase 7: Operations (Resilience & Lifecycle)
> **Goal**: Production readiness and ease of operation.
> **Status**: ⏳ Planned

- [ ] **Health Checks**: Active/Passive upstream health checking and outlier detection.
- [ ] **Lifecycle**: Graceful shutdown sequences and connection draining.
- [ ] **Admin API**: Runtime inspection endpoints and dynamic log level adjustment.
- [ ] **Tuning**: Configurable poll intervals and timeouts.

## Phase 8: Advanced Traffic Management
> **Goal**: Sophisticated routing and traffic control.
> **Status**: ⏳ Planned

- [ ] **DNS Upstreams**: `LOGICAL_DNS` support for dynamic backends.
- [ ] **Route Actions**: Direct responses, Redirects, Host rewrites, Path rewrites.
- [ ] **Traffic Control**: Rate limiting (local/distributed) and Fault injection.
- [ ] **Protocols**: gRPC transcoding, WebSocket support, and HTTP/2 upstream.
- [ ] **Extensibility**: WASM plugin interface.

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
- [ ] **[Safety] Unit Testing Gaps**: Low confidence in edge cases for new features. (Trigger: Before Phase 4)
- [ ] **[Safety] Integration Testing**: Risk of state desync during reloads. (Trigger: Before Phase 4)
- [ ] **[Safety] E2E Testing**: No validation against real network targets/kernels. (Trigger: Before v1.0 Release)
- [ ] **[Safety] Symlink Verification**: Verify ingest correctly follows symbolic links. (Trigger: Next Release)

### TD-2: Release Engineering & Safety
- [ ] **[Safety] CI Compatibility Fixtures**: Risk of breaking backward compatibility for older binaries. (Trigger: Before first public release)
- [ ] **[Arch] Governance Ownership**: Relay currently owns migration logic; should move to Governor. (Trigger: When Governor component is introduced)
- [ ] **[Safety] Strict Format Sniffing**: Verify file content type bytes, not just extension. (Trigger: Phase 4)

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

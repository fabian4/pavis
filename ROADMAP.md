# Pavis Roadmap

**Summary**
- **Total**: 27/70
- **Core Features**: 22/53
- **Technical Debt**: 5/17

> **Status**: Active
> **Focus**: Phase 4 (Security & Identity)
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
- [ ] [MUST] **Versioning Policy**: Define strict forward/backward compatibility rules for .pvs artifacts.

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
- [ ] [MUST] **CI/CD Pipeline**: Automated Multi-arch Docker builds (linux/amd64, linux/arm64) and Cargo publishing.

## Phase 3.5: Architecture Convergence & Boundary Hardening
> **Goal**: Harden pipeline stages and enforce strict component boundaries before xDS expansion. This introduces no new user-visible features.
> **Status**: ✅ Complete (Prerequisite for Phase 4)

- [x] **Typed Pipeline Stages**: Explicit, non-bypassable stages (Artifact -> CheckedArtifact -> RuntimeConfig -> ValidatedRuntimeConfig -> PVS).
- [x] **Dependency Inversion**: `pavis-relay` depends on ingest/codec traits, not concrete implementations.
- [x] **Plugin-Style Composition**: Feature-gated ingest/codec modules to keep binaries small and extensible.
- [x] **Boundary Enforcement**: Relay remains an execution/distribution engine; no semantic config interpretation.
- [x] **Convergence Gate**: No Phase 4 expansion until the above are complete and validated.
- [x] [MUST] **Test Harness**: Standardized integration test bed (Relay + Proxy + Mock Backend + Traffic Gen) for regression testing.

## Phase 4: Security & Identity (Critical Path)
> **Goal**: Enterprise-grade security capabilities essential for Zero-Trust environments.
> **Status**: ⏳ Planned (Promoted from Phase 6)

- [x] **TLS Termination**: Server-side TLS with single certificate per listener (No SNI).
- [ ] **mTLS (Mutual TLS)**: Client certificate validation + SPIFFE ID extraction.
- [ ] **Authorization (RBAC)**: Path/Method based policies (Deny-by-default).
- [ ] **Identity**: Integration with SPIRE/SPIFFE workload identities.

## Phase 5: Observability (Critical Path)
> **Goal**: Deep visibility into proxy behavior required for Operations.
> **Status**: ⏳ Planned (Promoted from Phase 7)

- [ ] **Prometheus Metrics**: Exporter with request, connection, and upstream dimensions.
- [ ] **Access Logs**: Configurable JSON/Text output to stdout or file.
- [ ] **Distributed Tracing**: OpenTelemetry (OTLP) integration for request tracing.
- [ ] **Runtime Stats**: Internal telemetry (config version, reload counts, payload size).

## Phase 6: Resilience & Discovery
> **Goal**: ensuring reliability in dynamic environments.
> **Status**: ⏳ Planned

- [ ] **Outlier Detection**: Passive health checks (eject 5xx pods).
- [ ] **Circuit Breaking**: Connection limits and max pending requests.
- [x] **DNS Discovery**: `StrictDns` (TTL-based) and `LogicalDns` (Lazy) support.
- [ ] **Active Health Checks**: Proactive `/healthz` pings.

## Phase 7: Operational Lifecycle
> **Goal**: Production readiness and ease of operation.
> **Status**: ⏳ Planned

- [ ] **Graceful Shutdown**: Connection draining sequences.
- [ ] **Admin API**: Runtime inspection endpoints and dynamic log level adjustment.
- [ ] **Tuning**: Configurable poll intervals and timeouts.

## Phase 8: xDS & Service Mesh Integration
> **Goal**: First-class integration with external control planes (Istio, Kuma).
> **Status**: ⚠️ Deferred (Blocked by Security & Observability)

- [ ] **xDS Ingest**: gRPC-based ADS (Aggregated Discovery Service) implementation.
- [ ] **xDS Codec**: Map LDS, RDS, CDS, and EDS into `RuntimeConfig`.
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
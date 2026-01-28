# Architecture: Frozen Data Plane Constitution

## 1. Axioms / Invariants
- **Frozen Data Plane** — Every semantic decision (routing, retries, TLS, RBAC, health, observability) **MUST** be resolved before serialization. No runtime component may construct or reinterpret policy.
- **No Runtime Interpretation** — The runtime **MUST NOT** parse text configs, infer defaults, evaluate scripts, or execute dynamic code. It only deserializes trusted `.pvs` artifacts produced by the compiler pipeline.
- **Atomic Reload Only** — Configuration transitions **MUST** occur via an all-or-nothing swap of the entire artifact. Partial updates, incremental edits, and mutable in-place structures are forbidden.
- **Fail-Closed Semantics** — Any validation error, environment violation, or artifact incompatibility **MUST** leave the runtime serving the last-known-good state. There is no graceful degradation, no fallback heuristics, and no best-effort mode.

## 2. Semantic Boundary
- **Compile-Time Decisions (Mandatory)**
  - Routing graphs, matcher predicates, header policies, rewrite plans, retry budgets, timeout values, circuit-breaker thresholds, health-check descriptions, TLS and mTLS settings, RBAC rules, telemetry field sets, upstream discovery modes, and listener layouts **MUST** be baked into `RuntimeConfig` before artifact sealing.
  - Any ambiguity, missing field, or optional behavior **MUST** be rejected by the codec. `Option<T>` in runtime types is reserved for explicit “enabled/disabled” states, never for “use default.”
- **Runtime Validation (Allowed Scope)**
  - The runtime may perform environment checks: file readability, key/cert presence, socket binding, DNS resolution reachability, OpenSSL initialization, and OS resource availability.
  - Runtime validation **MUST NOT** change semantics. If the environment check fails, the entire artifact is rejected and the previous artifact remains live.

## 3. Artifact Contract (`.pvs`)
- `.pvs` is a versioned binary ABI shared between codec, relay, and runtime. Layout changes **MUST** follow the roadmap gate for the versioning policy; until that contract is frozen, artifacts are considered unstable.
- Every artifact carries magic bytes, version metadata, and checksums. If any of these fail to match expectations, the runtime **MUST** abort the load.
- Corruption, mismatched architecture, or unsupported version **MUST** cause immediate rejection and a return to LKG. The relay is responsible for monotonic versioning; the runtime never mutates artifacts and never attempts repair.

## 4. Failure Semantics
- Reload success is binary. Either the artifact validates and atomically replaces the live state, or it is rejected with no partial side effects.
- Rollback is not a special path; refusal to load a new artifact simply keeps the current state. Manual rollback requires resealing or redelivering a prior artifact.
- LKG is authoritative. The runtime **MUST** keep serving the last applied artifact until a new artifact is proven valid. There is no shadow config, preview mode, or best-effort merging.
- There is no graceful degradation. If upstreams fail or artifacts are invalid, the runtime fails-closed and surfaces explicit errors instead of improvising behavior.

## 5. Relay Distribution Loop (Runtime)
- The runtime fetch loop is modeled as a pure FSM plus a driver that executes effects; no I/O occurs inside the FSM.
- Fetches are strictly single in-flight. The runtime long-polls with a fixed `wait_ms` and immediately re-issues the next poll on NoUpdate.
- `204`/`304` are NoUpdate and never trigger backoff. `410` is NeedResync and forces an unconditional refetch without backoff.
- `5xx` and network failures are transient and trigger capped exponential backoff.
- Deduplication is checksum-based: artifacts with the same ETag are never re-applied.

## 6. Runtime Bootstrap Phases
- `BootstrapPlan` is the single entry-point for runtime startup. `main.rs` only parses CLI/telemetry flags, then delegates to `BootstrapPlan::build` which materializes Pingora services.
- `build()` wires telemetry, runtime state, resolver/health monitors, listeners, agents, and admin endpoints **before** any service is started; `run()` is the only place that calls `Server::run_forever`.
- `listener::tls::TlsRuntime` owns TLS materialization. Pingora never sees raw paths—TLS certificates, private keys, and client-auth CA bundles are loaded and validated upfront with explicit `Optional`/`Required` semantics.
- All listener, telemetry, and agent wiring happens before `run()` is invoked, guaranteeing that a failed dependency (cert unreadable, metrics bind failure, etc.) aborts the boot rather than partially starting services.

## 7. Request Telemetry & Identity Handling
- **RequestTelemetry** — Every inbound request now owns a `RequestTelemetry` struct that captures the immutable `RequestId` plus the active tracing span. `RouterContext` delegates all span/state mutations to this struct so Pingora phases cannot forget to emit span updates or accidentally swap request ids mid-flight.
- **Phase Boundaries** — Router logic consults `RequestTelemetry` when recording route labels, upstream selections, and RBAC verdicts; only the telemetry struct may mutate tracing metadata, which keeps span lifetime aligned with request lifetime.
- **SPIFFE Identity** — `pavis-core` exposes a strong `SpiffeId` newtype that is propagated end-to-end (codec → runtime). Certificate extraction returns `Option<SpiffeId>` and RBAC principals match against this type, eliminating ambiguous `String` toggles and making it impossible to express an authenticated principal without an explicit identity payload.
- **Phase-Typed Router Context** — Request processing advances through `RoutingContext` → `RouteMatch` → `UpstreamAttempt`. Each phase exposes only the operations valid at that lifecycle step (route selection, RBAC, upstream selection), so pool permits, rewrites, and retries cannot be mutated out of order.


## 8. Endpoint Materialization
- `RuntimeState::from_config` eagerly resolves every upstream endpoint (Static, Strict, Logical) into concrete `SocketAddr`s during reload, so routing threads never perform DNS or touch filesystem state.
- The proxy/request-planning layer only accepts IP endpoints; attempting to build an upstream peer with a DNS endpoint now fails fast and logs the missing materialization so regressions cannot sneak back in.

## 9. Materialized Runtime Config
- Reload now produces a `MaterializedRuntimeConfig` that owns the pre-built router and upstream manager so request threads never see partially-constructed state.
- `RuntimeState` deref-coerces to the materialized struct and only exposes a single `ConfigVersion` newtype (non-zero) so metrics/admin surfaces no longer juggle `Option<u64>` defaults.
- Upstream clusters are segmented into `cluster/state.rs`, `cluster/health.rs`, `cluster/pool.rs`, and `cluster/tls.rs` modules, which keeps load balancing, health tracking, pooling, and TLS materialization isolated behind explicit APIs.

## 10. Health Monitoring & TLS Reuse
- TLS assets are materialized once per reload via `ClientIdentityMaterializer`. The helper produces Pingora-friendly `CertKey` handles, reqwest identities, and CA bundles, so both the data plane and health monitor reuse the exact same PEM parsing and validation logic.
- `Cluster` instances expose `health_identity()` and `health_root_certificates()` accessors that the health monitor consumes instead of re-reading files or constructing ad-hoc clients.
- `UpstreamHealthMonitor` is phase-driven: `HealthProbePlan` captures interval/path/client state, `Scheduler` enforces per-upstream cadence, and `Executor` fan-outs probes with `tokio::spawn`. This guarantees disabled health checks never schedule work and that intervals are honored regardless of runtime load.

## 11. Metrics Registry & Endpoint
- Metrics recording flows through `MetricsRegistry`, a thin wrapper around the Prometheus handle. All request/cluster/telemetry helpers share this registry so instrumentation stays centralized.
- The Prometheus endpoint is implemented by `telemetry::metrics::PrometheusEndpoint`, which depends on a pluggable `MetricsTransport`. The default transport binds a `tokio::net::TcpListener`, but tests can now inject custom transports without touching network sockets.
- Metrics exporting remains best-effort: if recorder installation fails or the listener cannot bind, the endpoint logs the error once and metrics stay disabled rather than partially-initialized.


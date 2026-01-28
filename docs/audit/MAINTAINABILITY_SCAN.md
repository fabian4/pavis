# Maintainability Scan (2026-01-28)

Update note:
- The runtime reload agent has been refactored into a pure FSM plus driver (`crates/pavis/src/agent/*`) with expanded unit/integration/E2E coverage. Findings here remain focused on the non-agent risk map.

## Top Risk Map
1. `crates/pavis/src/main.rs` – Bootstrap logic, telemetry wiring, relay setup, and Pingora lifecycle live in one 300+ line function that obscures ordering guarantees.
2. `crates/pavis/src/proxy/service/request_planning.rs` – Routing policy, retries, TLS decisions, and DNS resolution are interleaved, so every feature edit risks collateral damage.
3. `crates/pavis/src/proxy/context.rs` – `RouterContext` carries dozens of `Option` fields spanning multiple lifecycle phases, forcing callers to reason about invisible invariants.
4. `crates/pavis/src/proxy/service/io.rs` – The upstream peer builder mutates TLS settings, timeout policy, metrics, and logging in one routine with no extension hooks.
5. `crates/pavis/src/upstream/cluster.rs` – Cluster combines endpoint selection, health, circuit breakers, pool accounting, and TLS materials, creating a massive blast radius.
6. `crates/pavis/src/upstream/health.rs` – The health monitor schedules probes, constructs clients, parses TLS materials, and updates health atoms inside a single loop.
7. `crates/pavis/src/state.rs` – Runtime state re-validates regexes and upstreams on every reload and tracks config versions via `Option<u64>`, so there is no phase barrier.
8. `crates/pavis/src/telemetry/metrics.rs` – Metrics worker bundles Prometheus recorder installation with an ad-hoc HTTP server, preventing reuse or testing.
9. `crates/pavis/src/router.rs` – Route compilation mixes matcher zoning, regex compilation, and host bucketing, making future predicate changes fragile.
10. `crates/pavis/src/load.rs` – Load-time validation defers to runtime for semantic enforcement, so illegal states sneak into execution.

## Root Cause Clusters
- **C1 – Policy & Execution Coupling:** Runtime stages re-run semantic validation (timeouts, TLS policy, DNS) that belongs in codec/core.
- **C2 – Lifecycle/Option Soup:** Context structs are reused across phases with dozens of `Option` fields instead of phase-typed wrappers.
- **C3 – TLS Materialization Drift:** Multiple modules (listeners, health monitor, upstream peers) duplicate certificate parsing and client-auth policy.
- **C4 – Monolithic Modules:** Large files mix I/O, FSM logic, metrics, and orchestration, raising the cost of any change.
- **C5 – Missing Materialization Barriers:** Configs are recompiled/resolved inside hot paths because there is no `MaterializedRuntime` type (DNS resolution, regex caches, versioning).
- **C6 – Lacking Breakage Proofs:** Critical behavior (pool reuse hashing, rewrite skips, health scheduling) lacks dedicated tests, so regressions slip through silently.

## Findings

### Finding 1 – Bootstrap logic is a monolith (Severity **S1**, Clusters **C1, C4**)
- **Location:** `crates/pavis/src/main.rs:L120-L210`
- **Evidence:**
  ```rust
  let mut server_conf = ServerConf { daemon: false, ..Default::default() };
  match config.shutdown { .. }
  let runtime_state = pavis::state::RuntimeState::from_config(&config)?;
  let (telemetry, access_log_worker, metrics_worker, tracing_service) =
      Telemetry::new(&config.telemetry, Some(reload_handle.clone()));
  let config_agent = args.relay_url.as_ref().map(|relay| { .. ConfigAgent::new(..) .. });
  ```
- **Risk:** All startup concerns (Pingora config, telemetry reload hooks, agent wiring) share one scope with ad-hoc ordering, so adding a new component or failure path means auditing the entire function.
- **Concrete fix:** Introduce a `bootstrap` module with a `BootstrapPlan` struct that (1) materializes runtime state, (2) configs telemetry/metrics, (3) builds listener services, and (4) optionally wires the relay agent. `main()` should only parse CLI args, call `BootstrapPlan::build`, then `plan.run()`.
- **Minimal patch sketch:**
  ```rust
  pub struct BootstrapPlan { server: Server, telemetry: Arc<Telemetry>, services: Vec<Box<dyn Service>> }
  impl BootstrapPlan {
      pub fn build(cfg: &ValidatedRuntimeConfig, args: &Args) -> anyhow::Result<Self> { .. }
      pub fn run(self) { for svc in self.services { self.server.add_service(svc); } self.server.run_forever(); }
  }
  ```
- **Expected impact:** Startup changes (new services, alternate telemetry backends) become localized to the bootstrap module, drastically reducing the reviewer surface.

### Finding 2 – Listener TLS wiring reimplements policy (Severity **S1**, Clusters **C1, C3**)
- **Location:** `crates/pavis/src/main.rs:L224-L266`
- **Evidence:**
  ```rust
  match &listener.tls {
      pavis_core::TlsConfig::Enabled { cert_path, key_path, client_auth } => {
          let mut tls_settings = TlsSettings::intermediate(&cert_path.0, &key_path.0)?;
          match client_auth { pavis_core::ClientAuth::Optional { ca_path } => configure_client_auth(..,false)?, .. }
      }
  }
  ```
- **Risk:** Every listener mutates Pingora TLS settings directly, so adding OCSP stapling, ALPN, or new client-auth variants requires touching runtime code scattered across `main.rs`.
- **Concrete fix:** Move TLS materialization into `listener::tls::TlsRuntime`, accepting a `pavis_core::Listener` and returning a fully configured `TlsSettings`. Have `main.rs` simply call `TlsRuntime::build(listener)`.
- **Minimal patch sketch:**
  ```rust
  pub struct TlsRuntime<'a> { listener: &'a pavis_core::Listener }
  impl<'a> TlsRuntime<'a> {
      pub fn build(&self) -> anyhow::Result<TlsSettings> { match &self.listener.tls { .. } }
  }
  ```
- **Expected impact:** TLS policy remains in one place, so future compliance work or defaults (e.g., mandatory SAN checks) require a single diff.

### Finding 3 – Request planning mixes unrelated concerns (Severity **S1**, Clusters **C1, C4**)
- **Location:** `crates/pavis/src/proxy/service/request_planning.rs:L44-L210`
- **Evidence:**
  ```rust
  pub fn apply_route_headers(..) { .. }
  pub fn reuse_key_hash(..) { .. }
  pub fn calculate_path_rewrite(..) { .. tracing::warn!(..); }
  pub fn resolve_sni(..) { .. }
  ```
- **Risk:** Route header policy, retry math, TLS/SNI derivation, and path rewrites live together, so touching one concern (e.g., retries) risks breaking others.
- **Concrete fix:** Split this file into focused modules (`planning::id`, `planning::rewrite`, `planning::tls`, `planning::retry`) and expose a small facade consumed by IO. Move policy defaults (timeouts, retries) into codec/core so runtime only executes precomputed actions.
- **Minimal patch sketch:**
  ```rust
  pub mod planning {
      pub mod id { pub fn generate_req_id() -> RequestId { .. } }
      pub mod rewrite { pub fn apply(..) -> Option<Uri> { .. } }
      pub mod tls { pub fn sni(..) -> Option<Hostname> { .. } }
  }
  ```
- **Expected impact:** Routing changes no longer require touching TLS/resolution code, lowering merge conflicts and reviewer burden.

### Finding 4 – RouterContext is “Option soup” (Severity **S1**, Clusters **C2, C5**)
- **Location:** `crates/pavis/src/proxy/context.rs:L121-L151`
- **Evidence:**
  ```rust
  pub struct RouterContext {
      pub upstream_name: Option<UpstreamName>,
      pub pool_permit: Option<PoolPermit>,
      pub runtime_state: Option<Arc<RuntimeState>>,
      pub retry_ctx: Option<RetryContext>,
      pub rewritten_uri: Option<Uri>,
  }
  ```
- **Risk:** Callers must remember which fields are populated at which stage (routing, selection, retry). Forgetting to set a field silently propagates `None`, causing latent bugs.
- **Concrete fix:** Introduce phase-typed structs: `RoutingContext` (pre-route), `RouteSelection` (after router match), `DispatchContext` (after upstream selection). Each struct owns only relevant fields and exposes builders enforcing invariants.
- **Minimal patch sketch:**
  ```rust
  pub struct RoutingContext { req_id: RequestId, route: Option<RouteSelection> }
  pub struct DispatchContext { selection: RouteSelection, permits: PermitBundle, telemetry: RequestTelemetry }
  ```
- **Expected impact:** The compiler enforces lifecycle invariants, making retries, pool updates, and telemetry safer to evolve.

### Finding 5 – DNS resolution happens per request (Severity **S1**, Clusters **C1, C5**)
- **Location:** `crates/pavis/src/proxy/service/request_planning.rs:L280-L305`
- **Evidence:**
  ```rust
  match &endpoint.address {
      EndpointAddr::Dns { host, port } => {
          let mut addrs = (host.0.as_str(), port.0.get()).to_socket_addrs()?;
          match addrs.next() { Some(addr) => Ok(addr), None => Error::e_explain(..) }
      }
  }
  ```
- **Risk:** Every request performs synchronous DNS lookups, blocking Pingora workers and duplicating work already handled by the resolver/codec layers. It also mingles semantic validation (“DNS returned none”) with runtime execution.
- **Concrete fix:** Materialize `ResolvedEndpointAddr` when building `RuntimeState`, storing socket addresses (or resolver handles) in the upstream manager. Runtime code should consume these resolved endpoints without hitting DNS.
- **Minimal patch sketch:**
  ```rust
  pub enum ResolvedEndpointAddr { Socket(SocketAddr), Logical(LogicalName) }
  pub struct MaterializedEndpoint { addr: ResolvedEndpointAddr, original: Endpoint }
  ```
- **Expected impact:** DNS policy becomes deterministic per reload, and request threads simply pick pre-materialized addresses, improving predictability and readiness for async resolver swaps.

### Finding 6 – Upstream peer builder interleaves TLS, timeouts, and metrics (Severity **S1**, Clusters **C1, C4**)
- **Location:** `crates/pavis/src/proxy/service/io.rs:L319-L417`
- **Evidence:**
  ```rust
  let mut peer = HttpPeer::new(addr, use_tls, sni_string);
  if let Some(mode) = verify_mode { match mode { .. } }
  if let Some(cert_config) = cert { .. client_cert_key .. }
  if let Some(ca_config) = ca { .. peer.options.ca = Some(ca_bundle); }
  peer.options.idle_timeout = match upstream.pool.idle { .. };
  peer.options.connection_timeout = match upstream.pool.connect { .. };
  ```
- **Risk:** TLS options, timeout policy, pool metrics, and logging are welded together, so adding a single new TLS flag implies editing this giant match.
- **Concrete fix:** Introduce an `UpstreamPeerBuilder` type composed of focused steps: `apply_tls`, `apply_pool_timeouts`, `attach_metrics`, `finalize()`. Pull TLS materialization out so it reuses listener/health logic.
- **Minimal patch sketch:**
  ```rust
  let peer = UpstreamPeerBuilder::new(cluster, addr)
      .with_tls(cert, ca, verify_mode, sni)
      .with_timeouts(&upstream.pool, &ctx.retry_policy)
      .build();
  ```
- **Expected impact:** Future TLS or timeout changes edit isolated helpers rather than a sprawling function, reducing merge conflicts.

### Finding 7 – Cluster struct combines five subsystems (Severity **S1**, Clusters **C4, C5**)
- **Location:** `crates/pavis/src/upstream/cluster.rs:L361-L416`
- **Evidence:**
  ```rust
  pub struct Cluster {
      pub(crate) config: Upstream,
      pub(crate) rr_counter: AlignedCounter,
      state: ArcSwap<ClusterState>,
      health: Mutex<HealthState>,
      pool: PoolController,
      breaker: CircuitBreaker,
      client_cert_key: Option<Arc<CertKey>>,
      ca_bundle: Option<Arc<CaType>>,
  }
  ```
- **Risk:** Endpoint health, pool acquisition, circuit breaking, and TLS storage all share one type, so touching any subsystem risks deadlocks or inconsistent metrics.
- **Concrete fix:** Split into submodules: `cluster::state` (ArcSwap endpoints), `cluster::health` (outlier detector), `cluster::pool` (permits + metrics), `cluster::tls` (client cert/CA). `Cluster` becomes a thin facade delegating to these components.
- **Minimal patch sketch:**
  ```rust
  pub struct Cluster { selector: EndpointSelector, health: HealthTracker, pool: PoolHandle, tls: TlsMaterial }
  ```
- **Expected impact:** Each subsystem gains a dedicated file and test seam, making enhancements (e.g., new circuit breaker policy) safer.

### Finding 8 – Health monitor mixes scheduling, TLS parsing, and state updates (Severity **S1**, Clusters **C1, C3, C4**)
- **Location:** `crates/pavis/src/upstream/health.rs:L66-L110`
- **Evidence:**
  ```rust
  let should_run = match last_checks.get(name.as_str()) { .. };
  let client = match clients.get(name.as_str()) { Some(client) => client.clone(), None => match build_health_client(..) { .. } };
  for endpoint in cluster.current_endpoints() {
      let healthy = match probe_endpoint(..).await { .. };
      cluster.set_active_health(&endpoint.address, healthy);
  }
  ```
- **Risk:** Scheduling cadence, HTTP client construction, TLS identity parsing, and health updates all occur inline. Any change (e.g., new probe type) requires editing this tight loop.
- **Concrete fix:** Create a `HealthProbePlan` that owns interval + TLS materials and returns `ProbeJob`s. Split the worker into `Scheduler` (decides when) and `Executor` (performs probes). Reuse the same TLS materialization helper as runtime peers.
- **Minimal patch sketch:**
  ```rust
  struct HealthProbePlan { interval: Duration, client: Arc<reqwest::Client> }
  struct Scheduler { plans: HashMap<String, HealthProbePlan> }
  ```
- **Expected impact:** Health checks become extensible (gRPC, TCP, etc.) without rewriting scheduling logic, and TLS handling is shared.

### Finding 9 – RuntimeState lacks materialization barriers (Severity **S1**, Clusters **C2, C5**)
- **Location:** `crates/pavis/src/state.rs:L11-L37`
- **Evidence:**
  ```rust
  pub struct RuntimeState {
      pub config: ValidatedRuntimeConfig,
      pub router: Arc<Router>,
      pub upstream_manager: Manager,
      pub config_version: Option<u64>,
  }
  pub fn from_config(config: &ValidatedRuntimeConfig) -> anyhow::Result<Self> { .. router = Router::with_regex(config.routes.clone(), ..); let upstream_manager = Manager::new(&config.upstreams)?; }
  ```
- **Risk:** Every reload clones the entire config, recompiles regexes, and reinitializes upstream managers, while `config_version` can be `None`. There is no `MaterializedRuntimeConfig` to distinguish “compiled” vs. “live” state.
- **Concrete fix:** Introduce `MaterializedRuntimeConfig` (router cache, resolved endpoints, telemetry plan) plus a `ConfigVersion` newtype. RuntimeState should carry `MaterializedRuntimeConfig` and a non-null version so watchers can diff states safely.
- **Minimal patch sketch:**
  ```rust
  pub struct ConfigVersion(NonZeroU64);
  pub struct MaterializedRuntimeConfig { router: Arc<Router>, upstreams: MaterializedUpstreams }
  ```
- **Expected impact:** Reload cost drops, and version-aware consumers (metrics, agent) can assume `ConfigVersion` is present, simplifying telemetry.

### Finding 10 – Health TLS handling duplicates runtime logic (Severity **S2**, Clusters **C3, C4**)
- **Location:** `crates/pavis/src/upstream/health.rs:L221-L320`
- **Evidence:**
  ```rust
  if let TlsPolicy::Enabled { verify, ca, cert, .. } = &upstream.tls {
      if let pavis_core::UpstreamCa::File { path } = ca { let pem = std::fs::read(&path.0)?; builder = builder.add_root_certificate(reqwest::Certificate::from_pem(&pem)?); }
      if let pavis_core::ClientCert::Enabled { cert_path, key_path, chain } = cert { .. X509::stack_from_pem .. Pkcs12::builder() .. }
  }
  ```
- **Risk:** Health checks re-parse PEMs and build PKCS#12 bundles separately from cluster TLS setup, doubling the chance of drift and long-lived secrets in memory.
- **Concrete fix:** Extract a shared `ClientIdentityMaterializer` that produces reusable `CertKey`/`CaBundle` handles (for Pingora) and `reqwest::Identity` (for health). Both runtime and health monitor should depend on this helper.
- **Minimal patch sketch:**
  ```rust
  pub struct ClientIdentityMaterializer;
  impl ClientIdentityMaterializer {
      pub fn fetch(upstream: &Upstream) -> anyhow::Result<ClientIdentity> { .. }
  }
  ```
- **Expected impact:** TLS bugs are fixed once, and health monitor no longer opens files every interval.

### Finding 11 – Metrics worker couples recorder and HTTP transport (Severity **S2**, Clusters **C4, C6**)
- **Location:** `crates/pavis/src/telemetry/metrics.rs:L12-L90`
- **Evidence:**
  ```rust
  pub fn new(addr: SocketAddr) -> (Self, Option<MetricsRegistry>) {
      let builder = PrometheusBuilder::new();
      match builder.install_recorder() { Ok(handle) => (Self { addr, handle: Some(handle.clone()) }, Some(MetricsRegistry { .. })), Err(e) => .. }
  }
  async fn start_service(&mut self, ..) {
      let listener = tokio::net::TcpListener::bind(self.addr).await?;
      loop { tokio::select! { accept_result = listener.accept() => { tokio::spawn(async move { serve_metrics(stream, handle).await; }); } } }
  }
  ```
- **Risk:** There’s no seam to swap transports, inject auth, or unit-test metric rendering—the worker both installs the recorder and runs a raw TCP server.
- **Concrete fix:** Split into `MetricsRegistry` (installs recorder) and `PrometheusEndpoint` (HTTP service implementing `Service`). Inject the registry handle so tests can supply a fake transport.
- **Minimal patch sketch:**
  ```rust
  pub struct MetricsRegistry { handle: PrometheusHandle }
  pub struct PrometheusEndpoint { addr: SocketAddr, registry: MetricsRegistry }
  ```
- **Expected impact:** Adding TLS, auth, or alternative exporters becomes localized, and tests can assert that HTTP responses include expected metrics.

### Finding 12 – Router compilation mixes policy and mechanics (Severity **S2**, Clusters **C1, C4**)
- **Location:** `crates/pavis/src/router.rs:L42-L130`
- **Evidence:**
  ```rust
  for vhost in routes {
      for (index, route) in vhost.paths.iter().enumerate() {
          let has_predicates = !matches!(route.matcher.method, MethodPredicate::Any) || ..;
          match &route.matcher.path {
              PathMatch::Exact { path } if !has_predicates => { .. zones.push(RouteZone::ExactMap(map)); }
              PathMatch::Regex { path } => Some(Regex::new(&path.0)?);
          }
      }
  }
  ```
- **Risk:** The router simultaneously enforces policy (predicate ordering) and builds execution structures. There’s no typed separation between “validated matcher” and runtime plan, so adding a new predicate requires understanding both semantics and mechanics.
- **Concrete fix:** Define a `CompiledRoutePlan` type produced by the codec (or `MaterializedRuntimeConfig`) that already encodes zoning decisions. `router.rs` should only perform deterministic lookup over these plans.
- **Minimal patch sketch:**
  ```rust
  pub struct RoutePlan { host: HostKey, matcher: MatcherPlan, action: RouteAction }
  ```
- **Expected impact:** Predicate additions or policy tweaks can be made in the codec/core, while runtime router stays simple and deterministic.

## Proposed Tests (Breakage Proofs)
1. **`tests/proxy/request_planning.rs::pre_resolved_dns_is_used_once`** – Assert that routing reuses pre-resolved socket addresses across multiple requests, catching regressions where DNS falls back to per-request resolution.
2. **`tests/proxy/context.rs::routing_context_phase_transitions`** – Validate that `RoutingContext -> RouteSelection -> DispatchContext` transitions require explicit construction, preventing accidental use of unset permits.
3. **`tests/upstream/cluster_health.rs::outlier_ejection_and_recovery`** – Feed synthetic successes/failures into the new `HealthTracker` and ensure ejection timers expire deterministically.
4. **`tests/upstream/pool.rs::queue_metrics_recorded_once`** – Simulate permit acquisition timeouts and assert the queue metrics increment exactly once per rejection.
5. **`tests/telemetry/metrics_endpoint.rs::prometheus_endpoint_serves_response`** – Start the refactored metrics endpoint on a loopback port and verify it returns a well-formed Prometheus payload, ensuring transport abstraction works.
6. **`tests/health/probe_scheduler.rs::interval_respected`** – Use a mocked time source to prove the scheduler does not enqueue probes faster than configured intervals.

## Phased Execution Plan
1. **Phase 1 – Bootstrap & TLS Materialization**
   - Extract `BootstrapPlan` and `listener::tls::TlsRuntime` (Findings 1–2).
   - Dependency: none; unlocks later refactors by shrinking `main.rs`.
2. **Phase 2 – Proxy Context & Planning Split**
   - Introduce phase-typed contexts, request-planning submodules, `SpiffeId`, and `UpstreamPeerBuilder` (Findings 3–6).
   - Depends on Phase 1 (telemetry handles) to avoid merge churn.
3. **Phase 3 – Materialized Runtime & DNS Resolution**
   - Implement `MaterializedRuntimeConfig`, `ConfigVersion`, and `ResolvedEndpointAddr`; update router/planning accordingly (Findings 5 & 9 & 12).
   - Depends on Phase 2 (new planning facade) for clean integration.
4. **Phase 4 – Cluster & Health Subsystems**
   - Split cluster into submodules, add `HealthProbePlan`, share TLS materialization, and refactor metrics worker (Findings 7–11).
   - Depends on Phase 3 because materialized endpoints feed the cluster.
5. **Phase 5 – Test & Documentation Hardening**
   - Add proposed breakage-proof tests, update ARCHITECTURE.md to describe new phases/extension points, and run `make ci-local` + TLS/health E2Es.
   - Depends on prior phases to finalize new abstractions.

This sequencing keeps each change independently landable: Phase 1 shrinks `main.rs`; Phase 2 focuses on proxy internals; Phase 3 cements materialization; Phase 4 modernizes upstream and telemetry subsystems; Phase 5 seals the work with tests and docs.

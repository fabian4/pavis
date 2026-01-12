Phase 0: Instrumentation & Observability (Blocking)

1. **Goal**
   Expose the internal state of the connection pool to differentiate between "pool full" (resource exhaustion) and "pool fragmented" (broken reuse keys). Proof of fragmentation is the primary exit criteria.

2. **Concrete Code Changes**
   - **`crates/pavis/src/telemetry/metrics.rs`**:
     - Define new Prometheus metrics:
       - `upstream_pool_key_cardinality`: Gauge, labeled by `upstream`.
       - `upstream_connection_reused`: Counter, labeled by `upstream`.
       - `upstream_connection_new`: Counter, labeled by `upstream`, `reason` (e.g., "empty_pool", "expired").
   - **`crates/pavis/src/proxy/service.rs` (in `upstream_peer`)**:
     - Calculate the "Reuse Key" components tuple: `(endpoint_addr, sni_str, verify_mode, client_cert_id)`.
     - **Correction**: Do not track ALPN in the cardinality set as it generates excessive noise.
     - Implement a **Bounded Cardinality Tracker** (e.g., an `LruCache` with a hard capacity of 1000 per upstream) to estimate unique keys seen in the last 1m window.
     - Emit the `upstream_pool_key_cardinality` metric based on this tracker.
     - **Conditional Logging**: Add a `tracing::debug!` log dumping the reuse key tuple. Guard this with a rate-limiter (e.g., `governor` or simple atomic counter modulo) to prevent log flooding.

3. **Configuration / Schema Impact**
   - **Reuses Existing**: `RuntimeTelemetry` structure.
   - **Behavioral**: No schema changes.

4. **Metrics / Observability**
   - **`pavis_upstream_pool_key_cardinality_approx`**: High value (> number of backends) confirms fragmentation.
   - **`pavis_upstream_connection_reused_total`** vs **`pavis_upstream_connection_new_total`**: The ratio `reused / (reused + new)` determines the Reuse Rate.

5. **Risks / Guardrails**
   - **Memory Explosion**: Tracking cardinality of high-variance keys (e.g., raw Host headers) can consume memory. **Guardrail**: Use a bounded set (max 1024 entries) and saturating arithmetic for the gauge. If the set is full, report `1024+`.
   - **Performance**: Key string generation in the hot path. Use `Cow<str>` and avoid allocation where possible.

6. **Suggested PR Breakdown**
   - **PR 1**: Metrics definition and registry update.
   - **PR 2**: `upstream_peer` instrumentation with bounded cardinality tracking and rate-limited logging.

---

Phase 1: Allocator Replacement & Limits (Mitigation)

1. **Goal**
   Stabilize memory usage (RSS) and reduce lock contention during high-churn phases. Prevent OOM kills by strictly bounding upstream connection counts.

2. **Concrete Code Changes**
   - **`crates/pavis/Cargo.toml` & `src/main.rs`**:
     - Add `jemallocator`.
     - Configure `#[global_allocator]` with `background_thread: true` to aggressively purge dirty pages.
   - **`crates/pavis/src/proxy/service.rs`**:
     - In `upstream_peer`, read `Upstream.pool.max`.
     - **Clamp Logic**: If `pool.max` is `Unlimited` (None/0), inject a `ConnectionLimit::Limited(20_000)` into the `HttpPeer` options.
     - Log a `warn!` once per process lifetime (using `std::sync::Once`) if this clamp is applied, informing the operator that this is a **temporary stabilization measure** and `pool.max` should be configured explicitly.

3. **Configuration / Schema Impact**
   - **Behavioral Override**: Temporarily overrides `ConnectionLimit::Unlimited` to `20,000` (providing 2x headroom over 10k RPS to accommodate reuse failures).
   - **Configuration**: No schema change, but effectively deprecates "unlimited" behavior for this phase.

4. **Metrics / Observability**
   - **System Metrics**: `process_resident_memory_bytes` (RSS) should stabilize.
   - **Pingora Metrics**: `upstream_pending_connections` (if available via internal hooks) or inference via `503 Service Unavailable` rates if the limit is hit.

5. **Risks / Guardrails**
   - **Throttling**: The clamp is a safety net. It must be generous enough (20k+) to avoid breaking legitimate traffic before reuse is fixed.
   - **Exit Criteria**: This clamp logic must be slated for removal or converted to a safe default configuration once Phase 2 is validated.

6. **Suggested PR Breakdown**
   - **PR 1**: Switch to `jemalloc` and tune `malloc_conf`.
   - **PR 2**: Implement pool limit clamping in `upstream_peer`.

---

Phase 2: Deterministic Connection Reuse (The Fix)

1. **Goal**
   Decouple the transport connection key from per-request metadata (SNI) to restore O(1) pooling behavior.

2. **Concrete Code Changes**
   - **`crates/pavis-core/src/runtime/upstream.rs`**:
     - Add `canonical_sni: Option<Hostname>` to `TlsPolicy`.
     - Add `reuse_across_sni: bool` (default `false`) to `TlsPolicy`.
   - **`crates/pavis/src/proxy/service.rs`**:
     - **SNI Selection**: In `upstream_peer`:
       - If `canonical_sni` is `Some`, use it for the `HttpPeer` SNI.
       - If `SniName::Value` is set, use it.
       - If `SniName::Auto` is set AND `canonical_sni` is `None`: Warning log (rate limited) if `ctx.sni_override` varies.
     - **Group Key / Reuse Strategy**:
       - If `reuse_across_sni` is `true`:
         - Since `HttpPeer` relies on SNI for pooling, force the SNI used for the *connection pool* (not necessarily the handshake, if Pingora allows splitting them, otherwise both) to a stable value like the upstream IP string or a fixed placeholder.
         - **Note**: Standard `HttpPeer` binds SNI to the reuse key. To coalesce connections across requests with different `Host` headers, we MUST normalize the SNI passed to `HttpPeer::new()`.
       - **Invariant Check**: Only allow `reuse_across_sni` if `TlsPolicy` is `Enabled` AND `TlsVerify` is enabled. Unverified TLS reuse across SNIs is insecure.
   - **`crates/pavis/src/load_balancing.rs`**:
     - Ensure `select_endpoint` uses a stable algorithm (e.g., Consistent Hashing) if `LoadBalancer::RoundRobin` isn't sticky enough for the pool duration.

3. **Configuration / Schema Impact**
   - **New Fields**:
     - `tls.canonical_sni`: String. Used for handshake AND pool key.
     - `tls.reuse_across_sni`: Boolean. **Dangerous**. Opt-in only.
   - **Validation**: Reject `reuse_across_sni = true` if `tls.mode = Disabled`.

4. **Metrics / Observability**
   - **`upstream_pool_key_cardinality_approx`**: Should drop to `~ number of endpoints`.
   - **`upstream_connection_new`**: Should approach 0 in steady state.

5. **Risks / Guardrails**
   - **Security Risk**: `reuse_across_sni` allows connection coalescing. If the upstream server checks SNI for routing, traffic will go to the wrong tenant.
   - **Guardrail**: Clearly document that `reuse_across_sni` implies the backend certificate is valid for ALL hosts routed to this upstream (wildcard).

6. **Suggested PR Breakdown**
   - **PR 1**: Update `pavis-core` schema and Rkyv definitions (breaking change for config binary).
   - **PR 2**: Implement `canonical_sni` logic in `pavis`.
   - **PR 3**: Implement `reuse_across_sni` logic.

---

Phase 3: Controlled Configuration Exposure

1. **Goal**
   Expose a minimal, high-leverage subset of Pingora-facing parameters as user-configurable knobs to allow precise tuning without exposing unstable internals. Ensure effective configuration is logged for reproducibility.

2. **Concrete Code Changes**
   - **`crates/pavis-core/src/runtime/upstream.rs`**:
     - Add `tcp_keepalive` (optional duration), `tcp_nodelay` (optional boolean), and `recv_buffer_size` (optional size) to `PoolConfig`.
   - **`crates/pavis/src/proxy/service.rs`**:
     - In `upstream_peer`, map these new fields to `peer.options`:
       - `tcp_keepalive`: Defaults to 60s if unset (Pavis default), or user value.
       - `tcp_nodelay`: Defaults to true (Pingora default).
     - Log the final "Effective Configuration" for the upstream pool at `Info` level during initialization or first use (once), including implicit defaults.
   - **`crates/pavis/src/main.rs`**:
     - Expose server-wide settings in `RuntimeConfig`: `max_connections` and `threads`.
     - Pass these to `ServerConf` during startup.

3. **Configuration / Schema Impact**
   - **New Fields (Upstream)**:
     - `pool.tcp_keepalive`: Duration (Default: 60s).
     - `pool.tcp_nodelay`: Boolean (Default: true).
   - **New Fields (Global)**:
     - `server.max_connections`: Optional<u32>.
     - `server.threads`: Optional<usize>.
   - **Defaults**: Explicitly documented in schema (e.g., "60s", "true").

4. **Metrics / Observability**
   - **Startup Logs**: "Effective Upstream Config: idle_timeout=60s, keepalive=60s, nodelay=true".
   - **Runtime**: No new per-request metrics, but configuration state is visible.

5. **Risks / Guardrails**
   - **Misconfiguration**: Users might disable `nodelay` (latency spike) or set `keepalive` to 0 (leak).
   - **Guardrail**: Validate `tcp_keepalive > 1s` if enabled. Warn if `nodelay` is disabled.

6. **Suggested PR Breakdown**
   - **PR 1**: Update `pavis-core` schema with new fields.
   - **PR 2**: Map fields in `upstream_peer` and add effective config logging.
   - **PR 3**: Wire up global server settings in `main.rs`.

---

Phase 4: Post-Stability Hardening & Advanced Tuning

1. **Goal**
   After confirming stability via Phase 0-2 metrics, implement operational hardening and advanced tuning options to improve debuggability and resilience.

2. **Concrete Code Changes**
   - **Refine Defaults**:
     - Based on Phase 0 data, if `IdleTimeout::Disabled` proves risky, officially deprecate it in favor of `IdleTimeout::Default` (60s).
   - **Validation Tooling**:
     - Implement a "Config Lint" mode in the Pavis CLI (`pavis check config.yaml`).
     - specific checks:
       - Warn if `SniName::Auto` is used without `canonical_sni`.
       - Warn if `pool.max` is `Unlimited`.
   - **Adaptive Limits (Future)**:
     - Explore enabling Pingora's `background_service` for adaptive concurrency limiting if load tests show it's beneficial.

3. **Configuration / Schema Impact**
   - **Deprecation**: Mark `IdleTimeout::Disabled` as deprecated in documentation (non-breaking code change).
   - **Tooling**: New CLI subcommand `check`.

4. **Metrics / Observability**
   - **Lint Output**: Clear warnings for risky configurations.
   - **Benchmark Reports**: Publish reference benchmark results with the "Effective Config" logs attached.

5. **Risks / Guardrails**
   - **Complexity**: Adaptive limits can oscillate. Feature-flag them if introduced.
   - **User Friction**: Lint warnings might annoy users. Ensure they are suppressible or purely informational.

6. **Suggested PR Breakdown**
   - **PR 1**: CLI `check` command with initial lint rules.
   - **PR 2**: Documentation updates reflecting "Golden Profile" settings.
# Performance Remediation Implementation Plan

**Document Version**: 2.0
**Last Updated**: 2026-01-24
**Status Legend**: ❌ Not Started | ⏳ In Progress | ✅ Complete | ⚠️ Blocked | 🔄 Partially Implemented

---

## Phase 0: Instrumentation & Observability (Blocking)

**Status**: ❌ Not Started (0% complete)
**Priority**: P0 (Blocking)
**Blockers**: None
**Dependencies**: None

### **Current State Analysis**

**Existing Metrics** (related but insufficient):
- ✅ `pavis_upstream_pool_size` - tracks pool size but not key cardinality
- ✅ `pavis_connections_total` - counts connections but doesn't differentiate new vs. reused upstream connections
- ✅ `pavis_upstream_requests_total` - tracks upstream requests
- ✅ `pavis_upstream_pool_queue_depth` - tracks queued requests waiting for connections
- ✅ `pavis_upstream_pool_rejections_total` - tracks pool rejections by reason

**Missing Metrics** (critical for Phase 0 goals):
- ❌ `pavis_upstream_pool_key_cardinality` - not implemented
- ❌ `pavis_upstream_connection_reused_total` - not implemented
- ❌ `pavis_upstream_connection_new_total` - not implemented

**Code Analysis**:
- `crates/pavis/src/proxy/service.rs` (`upstream_peer` function):
  - ❌ No reuse key calculation or logging
  - ❌ No cardinality tracking mechanism
  - ❌ No bounded cardinality tracker (LruCache or similar)
- Connection pooling is handled internally by Pingora's `HttpPeer`, making reuse metrics difficult to expose without Pingora instrumentation

### 1. **Goal**
Expose the internal state of the connection pool to differentiate between "pool full" (resource exhaustion) and "pool fragmented" (broken reuse keys). Proof of fragmentation is the primary exit criteria.

### 2. **Concrete Code Changes**
- **`crates/pavis/src/telemetry/metrics.rs`**:
  - Define new Prometheus metrics:
    - `upstream_pool_key_cardinality`: Gauge, labeled by `upstream`.
    - `upstream_connection_reused`: Counter, labeled by `upstream`.
    - `upstream_connection_new`: Counter, labeled by `upstream`, `reason` (e.g., "empty_pool", "expired").
  - Add methods to `MetricsHandle`:
    - `record_pool_key_cardinality(upstream: &str, cardinality: usize)`
    - `record_connection_reused(upstream: &str)`
    - `record_connection_new(upstream: &str, reason: &str)`
- **`crates/pavis/src/proxy/service.rs` (in `upstream_peer`)**:
  - Calculate the "Reuse Key" components tuple: `(endpoint_addr, sni_str, verify_mode, client_cert_id)`.
  - **Correction**: Do not track ALPN in the cardinality set as it generates excessive noise.
  - Implement a **Bounded Cardinality Tracker** (e.g., an `LruCache` with a hard capacity of 1000 per upstream) to estimate unique keys seen in the last 1m window.
  - Emit the `upstream_pool_key_cardinality` metric based on this tracker.
  - **Conditional Logging**: Add a `tracing::debug!` log dumping the reuse key tuple. Guard this with a rate-limiter (e.g., `governor` or simple atomic counter modulo) to prevent log flooding.
  - **Challenge**: Pingora's internal pooling may not expose reuse events directly; may require wrapping or hooks.

### 3. **Configuration / Schema Impact**
- **Reuses Existing**: `RuntimeTelemetry` structure.
- **Behavioral**: No schema changes.

### 4. **Metrics / Observability**
- **`pavis_upstream_pool_key_cardinality_approx`**: High value (> number of backends) confirms fragmentation.
- **`pavis_upstream_connection_reused_total`** vs **`pavis_upstream_connection_new_total`**: The ratio `reused / (reused + new)` determines the Reuse Rate.

### 5. **Risks / Guardrails**
- **Memory Explosion**: Tracking cardinality of high-variance keys (e.g., raw Host headers) can consume memory. **Guardrail**: Use a bounded set (max 1024 entries) and saturating arithmetic for the gauge. If the set is full, report `1024+`.
- **Performance**: Key string generation in the hot path. Use `Cow<str>` and avoid allocation where possible.
- **Pingora Limitations**: Connection reuse is internal to Pingora; may need to infer from timing or instrument at lower level.

### 6. **Suggested PR Breakdown**
- **PR 1**: Metrics definition and registry update (`metrics.rs`).
- **PR 2**: Reuse key calculation and bounded cardinality tracker in `upstream_peer`.
- **PR 3**: Rate-limited debug logging for pool keys.

### 7. **Exit Criteria**
- [ ] Metrics deployed to production
- [ ] Dashboard showing pool key cardinality
- [ ] Baseline reuse rate established
- [ ] Fragmentation confirmed or ruled out

---

## Phase 1: Allocator Replacement & Limits (Mitigation)

**Status**: 🔄 Partially Implemented (50% complete)
**Priority**: P0 (Mitigation)
**Blockers**: None
**Dependencies**: Should run after Phase 0 diagnostics, but can start independently

### **Current State Analysis**

**Pool Limiting** (✅ IMPLEMENTED):
- ✅ `crates/pavis/src/upstream/cluster.rs` has robust pool limiting:
  - `PoolLimiter` with semaphore-based connection gating
  - Configurable via `Upstream.pool.max` (NonZeroU32, default 128)
  - Queue capacity and timeout support (`pool.queue.capacity`, `pool.queue.timeout_ms`)
  - Metrics: `pavis_upstream_pool_queue_capacity`, `pavis_upstream_pool_queue_depth`, `pavis_upstream_pool_size`, `pavis_upstream_pool_rejections_total`
- ✅ Pool permits tracked with RAII `PoolPermit` wrapper
- ✅ Rejection reasons: `queue_full`, `queue_timeout`, `closed`

**Allocator** (❌ NOT IMPLEMENTED):
- ❌ No `jemallocator` dependency in `Cargo.toml`
- ❌ No `#[global_allocator]` configuration in `main.rs`
- ⚠️ Using default system allocator (likely glibc malloc or platform default)

**Schema Note**:
- Current `ConnectionLimit` is always limited (`NonZeroU32`, default 128), so the "unlimited" case mentioned in the original plan doesn't exist in the current codebase.

### 1. **Goal**
Stabilize memory usage (RSS) and reduce lock contention during high-churn phases. Prevent OOM kills by strictly bounding upstream connection counts.

### 2. **Concrete Code Changes**
- **`crates/pavis/Cargo.toml`**:
  - Add `tikv-jemallocator = "0.6"` dependency.
- **`crates/pavis/src/main.rs`**:
  - Add global allocator configuration:
    ```rust
    #[cfg(not(target_env = "msvc"))]
    use tikv_jemallocator::Jemalloc;

    #[cfg(not(target_env = "msvc"))]
    #[global_allocator]
    static GLOBAL: Jemalloc = Jemalloc;
    ```
  - Configure jemalloc options via environment variable `MALLOC_CONF` (e.g., `background_thread:true,dirty_decay_ms:1000,muzzy_decay_ms:1000`).
  - Document jemalloc tuning in deployment guides.
- **`crates/pavis/src/proxy/service.rs`**:
  - ✅ Pool limiting already implemented via `cluster.acquire_pool_permit().await`.
  - ❌ No "unlimited override to 20k" logic needed (schema already enforces limits).
  - Consider logging effective pool limits at startup for observability.

### 3. **Configuration / Schema Impact**
- **No Schema Changes**: Pool limiting already enforced via `ConnectionLimit(NonZeroU32)`.
- **Behavioral**: jemalloc will replace system allocator, improving RSS behavior under churn.

### 4. **Metrics / Observability**
- **System Metrics**: `process_resident_memory_bytes` (RSS) should stabilize with jemalloc.
- **Existing Pool Metrics**: `pavis_upstream_pool_queue_depth`, `pavis_upstream_pool_rejections_total`.

### 5. **Risks / Guardrails**
- **Platform Compatibility**: jemalloc may not build on all targets (e.g., MSVC). Use conditional compilation.
- **Memory Overhead**: jemalloc may use more virtual memory than glibc malloc, but RSS should be lower.

### 6. **Suggested PR Breakdown**
- **PR 1**: Add `tikv-jemallocator` dependency and configure in `main.rs`.
- **PR 2**: Add startup logging for effective pool configuration.
- **PR 3**: Documentation updates for jemalloc tuning.

### 7. **Exit Criteria**
- [ ] `jemalloc` integrated and configured
- [ ] RSS stabilized under load (benchmark before/after)
- [x] Pool limit safety mechanism in place (DONE)
- [ ] Monitoring shows no OOM kills
- [ ] Documentation updated with jemalloc tuning guide

---

## Phase 2: Deterministic Connection Reuse (The Fix)

**Status**: ❌ Not Started (0% complete)
**Priority**: P0 (Root Cause Fix)
**Blockers**: Phase 0 must confirm fragmentation root cause
**Dependencies**: Phase 0 (diagnostic confirmation)

### **Current State Analysis**

**TLS Configuration** (from `pavis-core/src/runtime/upstream.rs`):
```rust
pub enum TlsPolicy {
    Disabled,
    Enabled {
        verify: TlsVerify,
        sni: SniName,
        cert: ClientCert,
        ca: UpstreamCa,
    },
}
```

**Missing Fields**:
- ❌ `canonical_sni: Option<Hostname>` - not present
- ❌ `reuse_across_sni: bool` - not present

**SNI Handling** (from `service.rs`):
- Current logic (lines 471-502):
  - SNI is resolved from `SniName` (Auto, Name, Disabled)
  - For `SniName::Auto`: uses `ctx.sni_override` or `endpoint_host`
  - For `SniName::Name`: uses explicit hostname
  - SNI is passed directly to `HttpPeer::new(addr, use_tls, sni_string)`
- ❌ No SNI normalization or canonicalization for pooling
- ⚠️ Connection pool key is determined by Pingora's `HttpPeer`, likely including SNI, causing fragmentation

**Load Balancing** (from `upstream/load_balance.rs`):
- Implements: `RoundRobin`, `Random`, `LeastRequest`
- Endpoint selection is per-request, not sticky to pool keys

### 1. **Goal**
Decouple the transport connection key from per-request metadata (SNI) to restore O(1) pooling behavior.

### 2. **Concrete Code Changes**
- **`crates/pavis-core/src/runtime/upstream.rs`**:
  - Modify `TlsPolicy::Enabled` to add:
    ```rust
    pub enum TlsPolicy {
        Disabled,
        Enabled {
            verify: TlsVerify,
            sni: SniName,
            cert: ClientCert,
            ca: UpstreamCa,
            canonical_sni: Option<Hostname>,  // NEW
            reuse_across_sni: bool,           // NEW (default false)
        },
    }
    ```
  - Update `rkyv` serialization attributes.
  - This is a **breaking schema change** requiring version bump.
- **`crates/pavis/src/proxy/service.rs`**:
  - **SNI Selection** in `upstream_peer` (around line 479):
    ```rust
    let sni_value = if let Some(canonical) = canonical_sni {
        // Use canonical SNI for both handshake and pool key
        Some(canonical.clone())
    } else {
        match sni {
            pavis_core::SniName::Name(name) => Some(name.clone()),
            _ => resolve_sni(sni, ctx.sni_override.as_ref(), endpoint_host.as_ref()),
        }
    };
    ```
  - **Reuse Strategy**:
    - If `reuse_across_sni` is true:
      - Use fixed SNI for pool key (e.g., canonical_sni or endpoint IP string)
      - **Security Check**: Only allow if `verify != Disabled`
      - Log warning if enabled
  - **Rate-limited Warning**: If `SniName::Auto` is used without `canonical_sni` and `ctx.sni_override` varies, log potential fragmentation.
- **`crates/pavis/src/upstream/load_balance.rs`**:
  - Review endpoint selection algorithms for pool key stability.
  - Consider adding consistent hashing if needed for deterministic routing.

### 3. **Configuration / Schema Impact**
- **New Fields** (add to `TlsPolicy::Enabled`):
  - `canonical_sni: Option<Hostname>`: Used for handshake AND pool key.
  - `reuse_across_sni: bool`: **Dangerous**. Opt-in only. Default `false`.
- **Validation** (in codec layer):
  - Reject `reuse_across_sni = true` if `verify = Disabled`.
  - Warn if `canonical_sni` is set but `sni != Auto`.
- **Breaking Change**: Schema version bump required.

### 4. **Metrics / Observability**
- **`pavis_upstream_pool_key_cardinality_approx`** (from Phase 0): Should drop to `~ number of endpoints`.
- **`pavis_upstream_connection_new_total`**: Should approach 0 in steady state.
- **Startup Logging**: Log effective SNI strategy per upstream.

### 5. **Risks / Guardrails**
- **Security Risk**: `reuse_across_sni` allows connection coalescing. If the upstream server checks SNI for routing (e.g., virtual hosting), traffic may be routed incorrectly or to wrong tenant.
- **Guardrail**:
  - Require `verify != Disabled` when `reuse_across_sni = true`.
  - Document that backend certificate must be valid for ALL hosts (wildcard or SAN).
  - Add runtime warning log when `reuse_across_sni` is enabled.

### 6. **Suggested PR Breakdown**
- **PR 1**: Update `pavis-core` schema with new TLS fields (breaking change).
- **PR 2**: Implement `canonical_sni` logic in `pavis` runtime.
- **PR 3**: Implement `reuse_across_sni` logic with security checks.
- **PR 4**: Update codecs to expose new fields.
- **PR 5**: Documentation updates with security warnings.

### 7. **Exit Criteria**
- [ ] Schema updated with new TLS fields
- [ ] Config validation enforces security invariants
- [ ] Pool key cardinality drops to expected levels (Phase 0 metrics)
- [ ] Connection reuse rate > 90% in steady state
- [ ] Load testing confirms behavior
- [ ] Security audit of `reuse_across_sni` feature
- [ ] Documentation updated with security warnings

---

## Phase 3: Controlled Configuration Exposure

**Status**: 🔄 Partially Implemented (40% complete)
**Priority**: P1 (Enhancement)
**Blockers**: None
**Dependencies**: None (can run in parallel with Phase 0-2)

### **Current State Analysis**

**Pool Struct** (from `pavis-core/src/runtime/upstream.rs`):
```rust
pub struct Pool {
    pub idle: IdleTimeout,
    pub connect: ConnectTimeout,
    pub max: ConnectionLimit,
    pub queue: PoolQueue,
}
```

**Existing Fields**:
- ✅ `idle: IdleTimeout` - mapped to `peer.options.idle_timeout`
- ✅ `connect: ConnectTimeout` - mapped to `peer.options.connection_timeout`
- ✅ `max: ConnectionLimit` - used for pool semaphore limits
- ✅ `queue: PoolQueue` - used for queue capacity and timeout

**Missing Fields**:
- ❌ `tcp_keepalive: Option<Duration>` - not in schema
- ❌ `tcp_nodelay: Option<bool>` - not in schema
- ❌ `recv_buffer_size: Option<u32>` - not in schema

**Peer Options Mapping** (from `service.rs`, lines 606-624):
```rust
peer.options.idle_timeout = match upstream.pool.idle { ... };
peer.options.connection_timeout = match upstream.pool.connect { ... };
peer.options.read_timeout = per_try_timeout;
peer.options.write_timeout = per_try_timeout;
```

**Missing Mappings**:
- ❌ `peer.options.tcp_keepalive` - not configured
- ❌ `peer.options.nodelay` - not configured (defaults to Pingora's default)
- ❌ No "effective configuration" logging

**Server Configuration** (from `main.rs`):
- ⚠️ `ServerConf` is created but server-wide max_connections and thread count are not exposed as config fields
- Uses hardcoded or Pingora defaults

### 1. **Goal**
Expose a minimal, high-leverage subset of Pingora-facing parameters as user-configurable knobs to allow precise tuning without exposing unstable internals. Ensure effective configuration is logged for reproducibility.

### 2. **Concrete Code Changes**
- **`crates/pavis-core/src/runtime/upstream.rs`**:
  - Extend `Pool` struct:
    ```rust
    pub struct Pool {
        pub idle: IdleTimeout,
        pub connect: ConnectTimeout,
        pub max: ConnectionLimit,
        pub queue: PoolQueue,
        pub tcp_keepalive: Option<Duration>,  // NEW
        pub tcp_nodelay: Option<bool>,        // NEW
        pub recv_buffer_size: Option<u32>,    // NEW
    }
    ```
  - Update defaults in `impl Default for Pool`.
- **`crates/pavis/src/proxy/service.rs`**:
  - In `upstream_peer`, map new fields to `peer.options`:
    ```rust
    if let Some(keepalive) = upstream.pool.tcp_keepalive {
        peer.options.tcp_keepalive = Some(Duration::from_millis(keepalive.0.get() as u64));
    }
    if let Some(nodelay) = upstream.pool.tcp_nodelay {
        peer.options.nodelay = nodelay;
    }
    if let Some(buffer_size) = upstream.pool.recv_buffer_size {
        peer.options.recv_buffer_size = Some(buffer_size as usize);
    }
    ```
  - Add "Effective Configuration" logging at first use per upstream:
    ```rust
    tracing::info!(
        upstream = %upstream_name.0,
        idle_timeout = ?peer.options.idle_timeout,
        connection_timeout = ?peer.options.connection_timeout,
        tcp_keepalive = ?peer.options.tcp_keepalive,
        tcp_nodelay = peer.options.nodelay,
        max_connections = upstream.pool.max.0.get(),
        "Effective upstream configuration"
    );
    ```
- **`crates/pavis/src/main.rs`**:
  - Expose server-wide settings in `RuntimeConfig` (or new `ServerConfig` struct):
    - `max_connections: Option<u32>`
    - `threads: Option<usize>`
  - Map to `ServerConf` during startup.

### 3. **Configuration / Schema Impact**
- **New Fields (Upstream `Pool` struct)**:
  - `tcp_keepalive: Option<Duration>` (Default: None, uses OS/Pingora default).
  - `tcp_nodelay: Option<bool>` (Default: None, uses Pingora default of true).
  - `recv_buffer_size: Option<u32>`.
- **New Fields (Global `RuntimeConfig` or `ServerConfig`)**:
  - `max_connections: Option<u32>`.
  - `threads: Option<usize>`.
- **Defaults**: Explicitly documented in schema and codecs.

### 4. **Metrics / Observability**
- **Startup Logs**: "Effective Upstream Config: idle_timeout=60s, keepalive=60s, nodelay=true, max_connections=128".
- **Runtime**: No new per-request metrics, but configuration state is visible in logs.

### 5. **Risks / Guardrails**
- **Misconfiguration**: Users might disable `nodelay` (latency spike) or set `keepalive` to 0 (potential leak).
- **Guardrail**:
  - Validate `tcp_keepalive > 1s` if set (in codec layer).
  - Warn if `nodelay = false` (log warning at startup).
  - Document performance implications.

### 6. **Suggested PR Breakdown**
- **PR 1**: Update `pavis-core` schema with new Pool fields.
- **PR 2**: Map fields in `upstream_peer` and add effective config logging.
- **PR 3**: Add server-wide settings to RuntimeConfig and wire up in `main.rs`.
- **PR 4**: Update codecs to expose and validate new fields.
- **PR 5**: Documentation updates with tuning guide.

### 7. **Exit Criteria**
- [ ] Schema updated with new Pool fields
- [ ] Validation logic for new parameters (in codec)
- [ ] Effective configuration logged at startup per upstream
- [ ] Server-wide max_connections and threads configurable
- [ ] Documentation updated with tuning guide
- [ ] Default values tested and validated
- [ ] Performance testing confirms tuning effectiveness

---

## Phase 4: Post-Stability Hardening & Advanced Tuning

**Status**: ❌ Not Started (0% complete)
**Priority**: P2 (Future)
**Blockers**: Phases 0-2 must be complete and validated
**Dependencies**: Phase 0-2 completion + production validation

### **Current State Analysis**

- ⚠️ `IdleTimeout::Disabled` is the current default (per `Pool::default()` in upstream.rs)
- ❌ No config lint tooling exists
- ❌ No benchmark suite for configuration validation

### 1. **Goal**
After confirming stability via Phase 0-2 metrics, implement operational hardening and advanced tuning options to improve debuggability and resilience.

### 2. **Concrete Code Changes**
- **Refine Defaults**:
  - Based on Phase 0 data, if `IdleTimeout::Disabled` proves risky (e.g., connection leaks), officially deprecate it in favor of `IdleTimeout::Enabled(Duration(60000))`.
  - Update `Pool::default()` to use `IdleTimeout::Enabled` as default.
  - Add deprecation warning in codecs if `IdleTimeout::Disabled` is explicitly set.
- **Validation Tooling**:
  - Implement a "Config Lint" mode in `pavctl` CLI (`pavctl check config.yaml`).
  - Specific checks:
    - Warn if `SniName::Auto` is used without `canonical_sni` (Phase 2 feature).
    - Warn if `pool.max` is set to extremely low values (< 10).
    - Warn if `IdleTimeout::Disabled` is used.
    - Warn if `reuse_across_sni = true` (security risk).
    - Validate queue capacity vs. max connections ratio.
- **Adaptive Limits (Future)**:
  - Explore enabling Pingora's `background_service` for adaptive concurrency limiting if load tests show it's beneficial.
  - Feature-flag experimental adaptive features.

### 3. **Configuration / Schema Impact**
- **Deprecation**: Mark `IdleTimeout::Disabled` as deprecated in documentation (non-breaking code change, but codec warnings).
- **Tooling**: New CLI subcommand `pavctl check`.

### 4. **Metrics / Observability**
- **Lint Output**: Clear warnings for risky configurations with remediation suggestions.
- **Benchmark Reports**: Publish reference benchmark results with the "Effective Config" logs attached.

### 5. **Risks / Guardrails**
- **Complexity**: Adaptive limits can oscillate. Feature-flag them if introduced.
- **User Friction**: Lint warnings might annoy users. Ensure they are suppressible or purely informational.

### 6. **Suggested PR Breakdown**
- **PR 1**: CLI `pavctl check` command with initial lint rules.
- **PR 2**: Update default Pool configuration to use IdleTimeout::Enabled.
- **PR 3**: Documentation updates reflecting "Golden Profile" settings.
- **PR 4**: Benchmark suite for configuration validation.

### 7. **Exit Criteria**
- [ ] Config lint tool implemented
- [ ] Deprecation warnings added for IdleTimeout::Disabled
- [ ] Production data validates default recommendations
- [ ] Benchmark suite established with reference configs
- [ ] Golden configuration profile documented
- [ ] Adaptive limits evaluated (optional)

---

## Cross-Phase Dependencies

```
Phase 0 (Instrumentation)
    ↓
    ├──→ Phase 2 (blocked until fragmentation confirmed)
    └──→ Phase 4 (blocked until data collected)

Phase 1 (Mitigation) ← can start independently
    ↓ (recommended sequence)
Phase 2 (Root Fix)
    ↓
Phase 4 (Hardening)

Phase 3 (Config Exposure) ← can run in parallel with all phases
```

---

## Overall Plan Status

| Phase | Status | Completion | Priority | Blocking Issues |
|-------|--------|------------|----------|-----------------|
| Phase 0 | ❌ Not Started | 0% | P0 | None - **START HERE** |
| Phase 1 | 🔄 Partial | 50% | P0 | Need jemalloc integration |
| Phase 2 | ❌ Not Started | 0% | P0 | Blocked by Phase 0 diagnostic confirmation |
| Phase 3 | 🔄 Partial | 40% | P1 | None - can proceed in parallel |
| Phase 4 | ❌ Not Started | 0% | P2 | Blocked by Phase 0-2 completion |

---

## Detailed Completion Tracking

### Phase 0 (0% complete)
- [ ] Define new metrics in `metrics.rs`
- [ ] Implement reuse key calculation
- [ ] Implement bounded cardinality tracker
- [ ] Add rate-limited debug logging
- [ ] Deploy and validate metrics

### Phase 1 (50% complete)
- [x] Pool limiting with semaphore (DONE)
- [x] Queue capacity and timeout (DONE)
- [x] Pool rejection metrics (DONE)
- [ ] Add jemalloc dependency
- [ ] Configure global allocator
- [ ] Benchmark RSS improvements
- [ ] Document jemalloc tuning

### Phase 2 (0% complete)
- [ ] Add `canonical_sni` field to schema
- [ ] Add `reuse_across_sni` field to schema
- [ ] Update rkyv serialization
- [ ] Implement SNI canonicalization logic
- [ ] Add security validation
- [ ] Update codecs
- [ ] Load testing and validation
- [ ] Security documentation

### Phase 3 (40% complete)
- [x] Basic timeout configuration (DONE)
- [ ] Add TCP keepalive field
- [ ] Add TCP nodelay field
- [ ] Add recv_buffer_size field
- [ ] Map new fields to peer.options
- [ ] Add effective config logging
- [ ] Add server-wide config exposure
- [ ] Update codecs
- [ ] Documentation and tuning guide

### Phase 4 (0% complete)
- [ ] Implement `pavctl check` command
- [ ] Add lint rules
- [ ] Refine IdleTimeout defaults
- [ ] Add deprecation warnings
- [ ] Create benchmark suite
- [ ] Document golden profile

---

## Immediate Next Actions

1. **Start Phase 0** (Blocking for Phase 2):
   - Implement connection reuse metrics
   - Deploy cardinality tracking
   - Confirm fragmentation hypothesis

2. **Complete Phase 1** (Independent):
   - Add jemalloc integration (low-hanging fruit)
   - Benchmark RSS improvements

3. **Advance Phase 3** (Parallel work):
   - Add TCP tuning parameters
   - Implement effective config logging

4. **Hold Phase 2** until Phase 0 confirms the root cause.

---

## Notes and Observations

1. **Good News**: Pool limiting is already well-implemented with robust metrics, better than originally planned.

2. **Critical Gap**: No visibility into connection reuse behavior (Phase 0 is essential).

3. **Architectural Insight**: Connection pooling is internal to Pingora's `HttpPeer`. We may need to either:
   - Hook into Pingora's connection lifecycle
   - Infer reuse from timing/metrics
   - Propose upstream changes to Pingora for instrumentation

4. **Schema Evolution**: Phase 2 and Phase 3 both require schema changes. Consider bundling into a single major version bump.

5. **Default Review**: Current `IdleTimeout::Disabled` default may be risky and should be evaluated in Phase 0/4.

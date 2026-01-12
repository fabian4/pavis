# Engineering Plan: Pavis Performance & Stability Remediation

## 1. Executive Summary

The observed concurrency collapse at 10k RPS and elevated memory usage (~500MB RSS) are primarily driven by **connection pool fragmentation** caused by non-deterministic peer keys, with memory allocator contention serving as a secondary exacerbating factor.

Current analysis indicates that `HttpPeer` construction likely varies the Server Name Indication (SNI) or peer identity per request (e.g., via `SniName::Auto` combined with high-cardinality Host headers). This forces Pingora to treat every request as a unique upstream destination, disabling connection pooling. The resulting 1:1 request-to-connection ratio triggers a storm of TLS handshakes, exhausting CPU and causing rapid object churn that overwhelms the system allocator.

This plan prioritizes stabilizing connection reuse keys to restore O(1) pooling behavior. Secondary steps include replacing the system allocator with **jemalloc** to handle residual churn and enforcing strict pool limits.

## 2. Root Cause Analysis

| Rank | Probability | Root Cause | Mechanism |
| :-- | :-- | :-- | :-- |
| **1** | Very High | **Pool Fragmentation (Unstable Keys)** | The connection pool key includes the SNI. If SNI varies per request (e.g., passing through raw `Host` headers), the pool fractures into thousands of single-use connections. **Result:** Full TLS handshake per request, 100% CPU usage, and latency collapse. |
| **2** | High | **Allocator Contention** | The massive object churn from creating/destroying `HttpPeer` and OpenSSL contexts per request overwhelms `malloc`. **Result:** Memory fragmentation (high RSS) and lock contention, magnifying the impact of #1. |
| **3** | Medium | **Implicit Pool Semantics** | `IdleTimeout::Disabled` may map to indefinite retention or default behavior that contradicts production needs, potentially leaking connections if not explicitly bounded. |
| **4** | Low | **Ephemeral Port Exhaustion** | A lagging indicator of #1. If reuse is 0%, the node will eventually run out of source ports (`EADDRNOTAVAIL`). |

## 3. Technical Strategy

### Connection Pooling Mechanics
Pingora’s `HttpPeer` pooling relies on a `reuse_hash` derived from the connection parameters. Crucially, this hash typically includes:
1.  **Destination Address** (`SocketAddr`)
2.  **SNI** (Server Name Indication)
3.  **TLS Settings** (Certificate verification mode, client certs)

**The Break**: If `SniName::Auto` is used with a backend serving wildcard domains (e.g., `*.example.com`), and the proxy blindly forwards the `Host` header as the SNI, the pool shards by subdomain. A backend serving 1,000 tenants will generate 1,000 separate connection pools, even if they all resolve to the same IP.

**The Fix**: To restore pooling, we must stabilize the inputs to `HttpPeer`. We need a **Deterministic Peer Key**. This means verifying that the SNI used for the upstream connection is constant (or low-cardinality) for a given upstream cluster, independent of the incoming request's specific Host header, unless explicit sharding is desired.

## 4. Remediation Plan

### Phase 0: Instrumentation & Observability (Blocking)

#### 1. Goal
Make the connection pool state visible to operators and engineers. The primary objective is to differentiate between "pool full" (resource exhaustion) and "pool fragmented" (broken reuse keys) scenarios in real-time.

#### 2. Concrete Changes
- **Module: Telemetry / Upstream**:
  - Implement a `PoolMonitor` service that periodically polls Pingora's internal pool state (if accessible) or tracks usage via a custom `ConnectionManager` wrapper.
  - Since Pingora does not expose granular pool cardinality by default, we will likely need to instrument `HttpPeer` creation to emit a debug log or metric representing the `reuse_hash` components.
- **Module: HttpPeer Construction**:
  - Add a debug-level log in `upstream_peer` that prints the exact parameters used for the connection key: `(IP, Port, SNI, ALPN, VerifyMode)`.
  - This allows us to grep logs to confirm if SNI is varying per request.
- **Module: Metrics Registry**:
  - Register new Prometheus counters:
    - `pavis_upstream_connection_reused_total`
    - `pavis_upstream_connection_new_total`
    - `pavis_upstream_pool_keys_total` (Cardinality check)

#### 3. Configuration Impact
- **Existing Config**: No changes to the configuration schema.
- **Runtime**: Telemetry level for `pingora` or `pavis` may need to be temporarily raised to `Debug` to capture the pool key logs.

#### 4. Metrics & Signals
- **Success Signal**: The `pavis_upstream_pool_keys_total` metric accurately reflects the number of active connection groups.
- **Failure Signal**: If this metric tracks 1:1 with `requests_total`, fragmentation is confirmed.

#### 5. Risks & Guardrails
- **Performance Overhead**: High-cardinality metrics (like tracking every unique SNI) can explode memory usage. Aggregate or sample if cardinality is extremely high.
- **Log Volume**: Debug logs in `upstream_peer` will be noisy at 10k RPS. Use sampling or conditional logging.

### Phase 1: Allocator Replacement & Limits (Mitigation)

#### 1. Goal
Stabilize the runtime environment by mitigating memory fragmentation caused by high churn and preventing Out-Of-Memory (OOM) kills through strict resource bounding.

#### 2. Concrete Changes
- **Module: Main / Cargo.toml**:
  - Add `jemallocator` dependency.
  - Set `#[global_allocator]` to `jemalloc` in `main.rs`.
  - Enable background threads (`background_thread: true`) in `malloc_conf` to assist with purging dirty pages.
- **Module: Upstream / Connection Pool**:
  - Modify the `HttpPeer` initialization logic to strictly respect `Upstream.pool.max`.
  - Implement a **temporary clamp**: If `Upstream.pool.max` is `Unlimited` (0 or None), enforce a hard cap of `10,000` connections per upstream to prevent runaway allocation during the remediation period. Log a warning when this clamp is applied.

#### 3. Configuration Impact
- **Temporary Override**: The automatic clamping of `Unlimited` pool sizes is a temporary safety measure.
- **Future Work**: This logic should eventually be removed once pooling is stable, returning full control to the user configuration.

#### 4. Metrics & Signals
- **Success Signal**: Resident Set Size (RSS) should stabilize or grow much slower than before.
- **Failure Signal**: Frequent "pool full" errors or 503s if the clamp is too aggressive for valid traffic.

#### 5. Risks & Guardrails
- **Clamping Side Effects**: Legitimate high-concurrency workloads might be throttled. The clamp value must be generous enough for 10k RPS assuming reasonable reuse.

### Phase 2: Deterministic Connection Reuse (The Fix)

#### 1. Goal
Restore O(1) connection pooling by ensuring that `HttpPeer` keys remain constant across requests for the same upstream cluster, decoupling the transport connection from per-request metadata.

#### 2. Concrete Changes
- **Module: Proxy / Upstream Selection**:
  - Refactor `upstream_peer` construction.
  - Implement logic to resolve a **Canonical SNI**:
    - If `SniName::Value` is set, use it (Already supported).
    - If `SniName::Auto` is set:
      - Check a new optional field `Upstream.tls.canonical_sni`.
      - If present, use it for the connection key.
      - If absent, fallback to `Host` header (current behavior) but emit a warning if the host varies.
- **Module: HttpPeer Configuration**:
  - Populate `peer.group_key` **conditionally**:
    - If `Upstream.tls.reuse_across_sni` (new boolean config) is `true`, set `group_key` to a tuple of `(IP, Port)`.
    - This forces Pingora to ignore SNI differences for pooling, relying on the fact that the backend cert covers all requested hosts.
- **Module: Endpoint Selection**:
  - Verify `select_endpoint` stability. Ensure that for a given upstream, the same endpoint IP is preferred for reuse unless load balancing policy dictates otherwise (e.g., utilize consistent hashing or sticky sessions if available/configured).

#### 3. Configuration Impact
- **New Fields**:
  - `Upstream.tls.canonical_sni` (Optional<String>): A fixed SNI to use for the handshake, regardless of the Host header.
  - `Upstream.tls.reuse_across_sni` (Boolean, default false): Explicit opt-in to unsafe pooling behavior.

#### 4. Metrics & Signals
- **Success Signal**: `connection_reused_total` increases rapidly; `connection_new_total` flattens near zero in steady state.
- **Validation**: `pool_keys_total` should equal `num_endpoints` (or `num_endpoints * num_unique_snis`), not `num_requests`.

#### 5. Risks & Guardrails
- **Security Invariant**: **Never** enable `reuse_across_sni` by default. It allows "Connection Coalescing" which is valid only if the cert is valid for both hosts and the backend allows it. Incorrect usage causes 421 Misdirected Request errors or security bypasses.

### Phase 3: Safe Defaults & Tuning

#### 1. Goal
Harden the system against resource leaks and "zombie" connections by establishing explicit, safe boundaries for connection lifecycles.

#### 2. Concrete Changes
- **Module: Runtime / Types**:
  - Review `IdleTimeout` deserialization. Ensure `Disabled` maps to an explicit "Close immediately" or a very short timeout (e.g., 0s or 1s) rather than relying on an ambiguous `None`.
- **Module: HttpPeer Options**:
  - Set `peer.options.tcp_keepalive` to a sensible default (e.g., 60s) if not explicitly configured.
  - This ensures that dead connections (e.g., due to firewall state table timeouts) are proactively detected and reaped by the pool.

#### 3. Configuration Impact
- **Defaults Change**: Existing configurations with "Disabled" timeouts may experience stricter connection closing behavior. This is intentional to prevent leaks.
- **Scope Control**: `max_connection_age` is deferred; we focus only on idle timeouts and keepalives for now.

#### 4. Metrics & Signals
- **Success Signal**: `active_connections` count drops promptly when load decreases.
- **Failure Signal**: Connections lingering in `ESTABLISHED` state long after traffic stops.

#### 5. Risks & Guardrails
- **Premature Closing**: Overly aggressive idle timeouts can hurt performance for bursty traffic. Defaults should be conservative (e.g., 60-120s), not aggressive (5s).

## 5. Validation Criteria

Success is defined by the following metrics under a 10k RPS load test:
1.  **Reuse Rate**: `connection_reused_total` / `requests_total` > **99%**.
2.  **Pool Cardinality**: `pool_keys` remains constant (approx `num_endpoints`), not growing with `requests_total`.
3.  **Stability**: New upstream connection rate converges toward **zero** in steady state.
4.  **Resource Usage**: RSS < 250MB, CPU linear with load (no exponential spikes).

## 6. What NOT To Change (Initially)

- **HttpPeer Customization**: Do not write a custom `Peer` struct yet; stick to configuring the existing `HttpPeer` correctly.
- **Architecture**: Do not rewrite the `Router` logic.
- **TLS Stack**: Remain on OpenSSL.
- **Schema**: Avoid breaking `.pvs` config changes; use existing fields or add optional ones.
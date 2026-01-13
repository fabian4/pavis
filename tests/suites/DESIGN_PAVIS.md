# Pavis Runtime (Data Plane) Suite: Design & Strength Review

## 1. Suite Goals
The Runtime Suite strictly validates the **Frozen Data Plane** contract. Its primary goal is to prove that the `pavis` binary:
1.  **Bootstraps** strictly from an immutable `.pvs` artifact.
2.  **Evolves** configuration dynamically via long-poll without process restarts (Hot Reload).
3.  **Survives** invalid or malicious configuration updates by falling back to the Last-Known-Good (LKG) state.
4.  **Executes** routing and resilience policies deterministically based *only* on the currently loaded artifact.

## 2. Core Invariants
*   **Invariant A (No-Drop):** Configuration updates MUST NOT interrupt active connections or drop new requests during the switch-over.
*   **Invariant B (LKG):** If a new artifact fails validation, the runtime MUST continue serving traffic using the previous valid configuration.
*   **Invariant C (Atomic Switch):** A request MUST be handled entirely by exactly one configuration version.
*   **Invariant D (Zero-Option):** The runtime MUST NOT infer defaults. Behavior is explicit in the artifact.

---

## 3. Case Design & Strength Analysis

### `10_bootstrap_static`
*   **Intent**: Startup from local file with no relay.
*   **Strength**: ✅ Solid. Proves basic PVS ingestion.

### `20_reload_norestart`
*   **Intent**: Primary long-poll hot-reload loop.
*   **Strength**: ⚠️ Needs Expansion. Proves "eventual" reload, but not "Zero-Drop" during transition.
*   **Expansion**: Add sequential request burst (100 reqs) during `publish`. Assert 100% status 200.

### `30_lkg_corrupt`
*   **Intent**: Rejection of binary corruption during reload.
*   **Strength**: ⚠️ Needs Expansion.
*   **Expansion**: Publish Valid V3 *after* rejection to prove the runtime isn't "stuck" in a failed state.

### `31_lkg_incompatible`
*   **Intent**: Rejection of semantically unsupported artifacts.
*   **Strength**: ✅ Solid (Scoped to current version tampering).

### `40_traffic_matcher`
*   **Intent**: Dynamic matching precedence evolution.
*   **Strength**: ✅ Solid. Strictly proves logic switch.

### `41_traffic_weighted`
*   **Intent**: Traffic splitting via weight changes.
*   **Strength**: ⚠️ Needs Expansion.
*   **Expansion**: Use deterministic "Weight Flip" (100/0 -> 0/100) to eliminate statistical flakiness.

### `42_traffic_regex`
*   **Intent**: Verify regex routing logic.
*   **Strength**: ✅ Solid. Proves regex compilation and matching priority.

### `43_traffic_headers`
*   **Intent**: Verify request/response header manipulation (Set/Add/Remove).
*   **Strength**: ✅ Solid. Proves Set/Add/Append/Remove logic for Request and Response headers.

### `44_traffic_actions`
*   **Intent**: Verify Redirect (3xx) and Direct Response (Static Body) actions.
*   **Strength**: ✅ Solid. Proves 3xx Redirect and 200 Direct response behaviors.

### `45_traffic_rewrite`
*   **Intent**: Verify Path Prefix and Host Header rewriting.
*   **Strength**: ✅ Solid. Proves Path Prefix replacement and Host header rewriting.

### `50_resilience_timeout` / `51_resilience_retry`
*   **Intent**: SLA and retry policy enforcement.
*   **Status**: ⏳ Planned.

### `60_security_tls`
*   **Intent**: Upgrading cleartext upstream to TLS.
*   **Strength**: ✅ Solid for TLS origination. SNI behavior is covered by `66_security_tls_sni_auto`.

### `61_security_termination`
*   **Intent**: Verify Server-side TLS termination (HTTPS Listener).
*   **Strength**: ✅ Solid. Proves HTTPS listener and enforced mTLS.

### `62_security_mtls_handshake`
*   **Intent**: Inbound mTLS handshake enforcement (Required/invalid CA).
*   **Strength**: ✅ Solid. Covers no-cert, valid cert, and unknown CA cases.

### `63_security_rbac_spiffe`
*   **Intent**: SPIFFE identity match authorization.
*   **Strength**: ✅ Solid. Covers match, mismatch, and no identity scenarios.

### `64_security_rbac_prefix`
*   **Intent**: SPIFFE prefix authorization.
*   **Strength**: ✅ Solid. Ensures prefix match enforcement and deny-by-default.

### `65_security_mtls_outbound`
*   **Intent**: Outbound mTLS with client cert and CA verification.
*   **Strength**: ✅ Solid. Exercises client cert + CA bundle wiring.

### `66_security_tls_sni_auto`
*   **Intent**: Auto SNI derivation and fail-fast for invalid Auto SNI configs.
*   **Strength**: ✅ Solid. Validates DNS-based Auto SNI and rejects IP endpoints without override.

### 67_security_mtls_chain_mode
*   **Intent**: Client cert chain_mode handling (embedded vs default none).
*   **Strength**: ✅ Solid. Ensures embedded chains are explicit and default is leaf-only.

### 70_obs_metrics
*   **Intent**: Verify Prometheus metrics exposition and correctness.
*   **Strength**: ✅ Solid. Validates request counts, latencies, and label correctness (route patterns).

### 71_obs_access_log
*   **Intent**: Verify structured access logging to file.
*   **Strength**: ✅ Solid. Validates JSON format and presence of all metadata fields (req_id, upstream timing).

### 72_obs_tracing_context
*   **Intent**: Verify W3C trace context propagation to upstreams.
*   **Strength**: ✅ Solid. Ensures `traceparent` headers are injected when tracing is enabled.

### 73_obs_cardinality
*   **Intent**: Verify cardinality protection for metrics.
*   **Strength**: ✅ Solid. Proves that unmatched routes drop labels and increment the drop counter instead of polluting metrics.

---

## 4. Implementation Principles
*   **Isolation**: Every request MUST include `X-Pavis-Test-Run` and `X-Pavis-Test-Case`.
*   **Black-Box**: Assert behavior via HTTP status/body or mock-upstream `/echo`.
*   **Mode-Agnostic**: Scripts use `get_sut_id` and `stop_sut` to work in both Binary and Docker modes.

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

### `30_lkg`
*   **Intent**: Unified LKG enforcement covering corrupt payloads *and* incompatible protocol versions.
*   **Strength**: ✅ Solid. Sequentially proves corrupt/incompatible artifacts are rejected and recovery to the next valid artifact succeeds.

### `40_traffic_routing_semantics`
*   **Intent**: Exercise matcher precedence, regex routing, header policies, route actions, and rewrites in a single artifact.
*   **Strength**: ✅ Solid. Multiple request variants prove each semantic without rebooting the runtime.

### `41_traffic_weighted`
*   **Intent**: Traffic splitting via weight changes.
*   **Strength**: ⚠️ Needs Expansion.
*   **Expansion**: Use deterministic "Weight Flip" (100/0 -> 0/100) to eliminate statistical flakiness.

### `50_resilience_timeout` / `51_resilience_retry`
*   **Intent**: SLA and retry policy enforcement.
*   **Status**: ⏭️ Skipped (feature not implemented in runtime yet).

### `52_resilience_outlier_detection`
*   **Intent**: Passive ejection after consecutive 5xx responses with timed re-admission.
*   **Strength**: ✅ Solid. Exercises failure counter, ejection window, and recovery.

### `53_resilience_active_health_check`
*   **Intent**: Active probes mark endpoints unhealthy and recover after config update.
*   **Strength**: ✅ Solid. Validates probe path semantics and health state transitions.

### `54_resilience_circuit_breaker`
*   **Intent**: Enforce in-flight and pending limits with 503 on overflow.
*   **Strength**: ✅ Solid. Uses concurrent long requests to force breaker rejection.

### `60_security_tls`
*   **Intent**: Upgrading cleartext upstream to TLS with custom CA verification.
*   **Strength**: ⏭️ Skipped under rustls backend (upstream limitation: per-peer CA not supported).

### `61_security_inbound_mtls`
*   **Intent**: HTTPS termination with client certificate validation and unknown-CA rejection.
*   **Strength**: ⏭️ Skipped under rustls backend (upstream limitation: inbound mTLS not supported).

### `62_security_rbac_spiffe`
*   **Intent**: SPIFFE identity match authorization.
*   **Strength**: ✅ Solid. Covers match, mismatch, and no identity scenarios.

### `63_security_rbac_prefix`
*   **Intent**: SPIFFE prefix authorization.
*   **Strength**: ✅ Solid. Ensures prefix match enforcement and deny-by-default.

### `64_security_mtls_outbound`
*   **Intent**: Outbound mTLS with client cert and CA verification.
*   **Strength**: ⏭️ Skipped under rustls backend (upstream limitation: per-peer CA and client cert not supported).

### `65_security_tls_sni_auto`
*   **Intent**: Auto SNI derivation and fail-fast for invalid Auto SNI configs.
*   **Strength**: ⏭️ Skipped under rustls backend (upstream limitation: per-peer CA verification required).

### `66_security_mtls_chain_mode`
*   **Intent**: Client cert chain_mode handling (embedded vs default none).
*   **Strength**: ⏭️ Skipped under rustls backend (upstream limitation: client cert presentation not supported).

### `70_obs_metrics`
*   **Intent**: Verify Prometheus metrics exposition plus label-cardinality protection.
*   **Strength**: ✅ Solid. Proves counters/gauges for matched routes and verifies drops when unmatched paths exceed label limits.

### `71_obs_access_log`
*   **Intent**: Verify structured access logging to file.
*   **Strength**: ⏭️ Skipped (binary mode access log verification is inconsistent due to flush/sync timing).

### `72_obs_tracing_context`
*   **Intent**: Verify W3C trace context propagation to upstreams.
*   **Strength**: ⏭️ Skipped (dynamic tracing sampling updates are not applied reliably yet).

### `80_obs_cross_consistency`
*   **Intent**: Verify metrics, access logs, and response headers agree on the same request identifiers.
*   **Strength**: ⏭️ Skipped (trace ID propagation check is failing in binary mode).

---

## 4. Implementation Principles
*   **Isolation**: Every request MUST include `X-Pavis-Test-Run` and `X-Pavis-Test-Case`.
*   **Black-Box**: Assert behavior via HTTP status/body or mock-upstream `/echo`.
*   **Mode-Agnostic**: Scripts use `get_sut_id` and `stop_sut` to work in both Binary and Docker modes.

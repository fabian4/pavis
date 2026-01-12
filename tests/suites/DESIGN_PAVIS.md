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
*   **Status**: 🚧 Skipped (Verification Pending).

### `43_traffic_headers`
*   **Intent**: Verify request/response header manipulation (Set/Add/Remove).
*   **Status**: 🚧 Skipped (Verification Pending).

### `44_traffic_actions`
*   **Intent**: Verify Redirect (3xx) and Direct Response (Static Body) actions.
*   **Status**: 🚧 Skipped (Verification Pending).

### `45_traffic_rewrite`
*   **Intent**: Verify Path Prefix and Host Header rewriting.
*   **Status**: 🚧 Skipped (Verification Pending).

### `50_resilience_timeout` / `51_resilience_retry`
*   **Intent**: SLA and retry policy enforcement.
*   **Status**: ⏳ Planned.

### `60_security_tls`
*   **Intent**: Upgrading cleartext upstream to TLS.
*   **Strength**: ⚠️ Needs Expansion.
*   **Expansion**: Assert `tls.sni` in upstream response matches configuration.

### `61_security_termination`
*   **Intent**: Verify Server-side TLS termination (HTTPS Listener).
*   **Status**: 🚧 Skipped (Verification Pending).

---

## 4. Implementation Principles
*   **Isolation**: Every request MUST include `X-Pavis-Test-Run` and `X-Pavis-Test-Case`.
*   **Black-Box**: Assert behavior via HTTP status/body or mock-upstream `/echo`.
*   **Mode-Agnostic**: Scripts use `get_sut_id` and `stop_sut` to work in both Binary and Docker modes.

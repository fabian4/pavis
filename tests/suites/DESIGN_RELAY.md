# Pavis Relay Suite: Design & Strength Review

## 1. Suite Goals
The Relay Suite validates the **Control Plane** correctness of the `pavis-relay` binary. It treats the relay as a black-box HTTP artifact distribution engine. Its primary goal is to prove that the relay:
1.  **Accepts** opaque configuration artifacts via a publication API.
2.  **Distributes** these artifacts to subscribers using efficient Long-Polling semantics.
3.  **Scales** functionally to support fanout to multiple subscribers without data loss.
4.  **Persists** state across process restarts.

## 2. Core Invariants
*   **R1 (Opaque):** The relay stores and serves artifacts byte-for-byte identical to what was published.
*   **R2 (Versioned):** Every artifact is associated with a unique, monotonic version.
*   **R3 (Blocking):** Subscribers requesting the *current* version block until a *new* version is available.
*   **R4 (Fanout):** A single publication event propagates to ALL active long-polling subscribers.
*   **R5 (Concurrency):** Simultaneous operations do not result in corrupted state.
*   **R6 (Persistence):** A restarted relay serves the Last-Known-Good (LKG) artifact immediately.

---

## 3. Case Design & Strength Analysis

### `10_contract_opaque`
*   **Intent**: Basic create-read cycle.
*   **Strength**: ✅ Solid. Uses byte-level `cmp` oracle.

### `11_contract_republish`
*   **Intent**: Monotonicity enforcement.
*   **Strength**: ✅ Solid. Proves 409 Conflict logic.

### `20_longpoll_wait`
*   **Intent**: Subscriber blocks until update.
*   **Strength**: ⚠️ Needs Expansion. Proves "eventual" unblock, but not that it was actually blocked.
*   **Expansion**: Verify background subscriber process liveness *before* publishing.

### `21_longpoll_timeout`
*   **Intent**: Subscriber waits for full timeout if no change.
*   **Strength**: ✅ Solid. Uses temporal oracle.

### `30_fanout_multi`
*   **Intent**: Broadcast to multiple subscribers.
*   **Strength**: ✅ Solid. Recently hardened with metrics-based readiness polling.

### `31_fanout_late`
*   **Intent**: Late subscriber catch-up.
*   **Strength**: ✅ Solid. Proves non-blocking behavior for stale clients.

### `40_concurrency_rapid`
*   **Intent**: High-frequency update stress.
*   **Strength**: ⚠️ Needs Expansion. Only checks final state.
*   **Expansion**: Assert version headers seen by subscriber are strictly monotonic (never decreasing).

### `50_persistence_recovery`
*   **Intent**: State recovery across restarts.
*   **Strength**: ✅ Solid. Verified in both Binary and Docker modes.

### `60_robustness_reconnect`
*   **Intent**: Subscriber disconnection and immediate catch-up.
*   **Strength**: ✅ Solid.

### `70_limits_oversize` / `71_limits_empty`
*   **Intent**: Payload size enforcement.
*   **Strength**: ✅ Solid. Proves rejection of invalid sizes.

---

## 4. Implementation Principles
*   **Isolation**: Each case runs against a fresh, isolated `pavis-relay` instance.
*   **Determinism**: Wait for readiness via `/health` or `/status` before logic.
*   **Black-Box**: Interact solely via HTTP API and observable process state.
# Pavis Runtime Test Suite: Architecture & Design Specification

## 1. Runtime Suite Goals
The Runtime Suite exists to strictly validate the **Frozen Data Plane** contract. Its primary goal is to prove that the `pavis` binary:
1.  **Bootstraps** strictly from an immutable `.pvs` artifact.
2.  **Evolves** configuration dynamically via long-poll without process restarts (Hot Reload). This is the **normal operating mode** of the system.
3.  **Survives** invalid or malicious configuration updates by falling back to the Last-Known-Good (LKG) state.
4.  **Executes** routing and resilience policies deterministically based *only* on the currently loaded artifact.

This suite isolates the runtime engine. It mocks the control plane (`pavis-mock-relay`) and the backend (`pavis-mock-upstream`) to treat the runtime as a pure function of `Config + Time = Behavior`. Most test cases follow the pattern: **Bootstrap V1 → Publish V2 → Assert Behavior Switch**. Restart-based validation is disallowed except for explicit crash recovery tests.

## 2. Core Runtime Invariants
Every test case in this suite must uphold or verify the following invariants:

*   **Invariant A (The No-Drop Guarantee):** Configuration updates MUST NOT interrupt active connections or drop new requests during the switch-over.
*   **Invariant B (The LKG Guarantee):** If a new configuration artifact fails validation (integrity or semantic), the runtime MUST continue serving traffic using the previous valid configuration.
*   **Invariant C (The Atomic Switch):** A request MUST be handled entirely by exactly one configuration version. There is no "mixed" state.
*   **Invariant D (The Zero-Option Assumption):** The runtime MUST NOT infer defaults. Behavior is explicit in the artifact.

## 3. Case Taxonomy
The suite is organized into four critical zones of verification:

1.  **Lifecycle & Governance (40%)**: Proof of hot-reload, LKG, and startup safety.
2.  **Traffic Management (30%)**: Proof that routing/splitting logic changes correctly under reload.
3.  **Resilience & Policies (20%)**: Proof that timeouts, retries, and circuit breakers behave as configured.
4.  **Protocol & Security (10%)**: TLS termination and origination semantics.

---

## 4. Detailed Case Design

### Zone 1: Lifecycle & Governance

#### `lifecycle_01_bootstrap_static`
*   **Category:** Bootstrap & Initial Load
*   **What is tested:** Ability to start with a local file and no relay connection. **Bootstrap-only behavior.**
*   **Initial State:** `pavis` started with `--config initial.pvs`. No relay URL provided.
*   **Traffic Pattern:** Simple health check and `/echo` request.
*   **Assertions:** Process starts, port opens, traffic flows to backend defined in `initial.pvs`.
*   **Invariants:** D (Zero-Option) — validates that behavior is derived strictly from the artifact without implicit defaults.
*   **Notes:** Hot-reload is **not** exercised in this case.

#### `lifecycle_02_hot_reload_basic`
*   **Category:** Reload Semantics
*   **What is tested:** The primary long-poll update loop.
*   **Initial State:** `pavis` started with `v1.pvs` (Routes to Backend A). Connected to `mock-relay`.
*   **Reload Sequence:** Publish `v2.pvs` (Routes to Backend B) to `mock-relay`.
*   **Traffic Pattern:** Continuous curl loop targeting the listener.
*   **Assertions:**
    1. Traffic initially hits Backend A.
    2. After publish + poll interval, traffic shifts to Backend B.
    3. **Crucial:** `pavis` PID remains constant (no restart).
*   **Invariants:** A (No-Drop), C (Atomic Switch).

#### `lifecycle_03_lkg_corruption`
*   **Category:** Failure & LKG
*   **What is tested:** Rejection of binary corruption during reload.
*   **Initial State:** Serving `v1.pvs` (valid).
*   **Reload Sequence:** Publish `corrupt.pvs` (random bytes or bad checksum) to `mock-relay`.
*   **Traffic Pattern:** Consistent requests during the failure injection.
*   **Assertions:**
    1. Traffic continues to flow using `v1` configuration.
    2. Runtime logs error regarding `.pvs` magic bytes or checksum.
    3. Process does *not* crash or stop polling.
*   **Invariants:** B (LKG).

#### `lifecycle_04_lkg_semantic_invalidity`
*   **Category:** Failure & LKG
*   **What is tested:** Rejection of structurally valid but semantically unsupported artifacts.
*   **Initial State:** Serving `v1.pvs` (valid).
*   **Reload Sequence:** Publish `v2_unsupported.pvs` (valid binary format, but contains an unsupported feature flag or version header mismatch that the runtime can detect).
*   **Traffic Pattern:** Consistent requests.
*   **Assertions:**
    1. Runtime rejects load after internal validation.
    2. Traffic stays on `v1`.
*   **Invariants:** B (LKG).
*   **Notes:** Planned – pending runtime-level semantic validation support.

### Zone 2: Traffic Management

#### `traffic_01_matcher_evolution`
*   **Category:** Traffic Behavior Under Reload
*   **What is tested:** Changing route matching precedence dynamically.
*   **Initial State:** `v1.pvs`: Match prefix `/api` -> Backend A.
*   **Reload Sequence:** Publish `v2.pvs`: Match exact `/api/v2` -> Backend B; Match prefix `/api` -> Backend C.
*   **Traffic Pattern:** Request specifically to `/api/v2`.
*   **Assertions:**
    1. Pre-reload: Hits Backend A (caught by prefix `/api`).
    2. Post-reload: Hits Backend B (caught by exact match).
*   **Invariants:** C (Atomic Switch).

#### `traffic_02_weighted_shift`
*   **Category:** Traffic Behavior Under Reload
*   **What is tested:** Traffic splitting via weight changes.
*   **Initial State:** `v1.pvs`: 100% Backend A.
*   **Reload Sequence:** Publish `v2.pvs`: 50% Backend A, 50% Backend B.
*   **Traffic Pattern:** Burst of requests (N > 100).
*   **Assertions:**
    1. Pre-reload: 100/0 distribution.
    2. Post-reload: Distribution falls within statistical tolerance (e.g., 40-60%) OR verify using deterministic hashing if enabled.
*   **Invariants:** A (No-Drop).

### Zone 3: Resilience & Policies

#### `resilience_01_timeout_tightening`
*   **Category:** Traffic Behavior Under Reload
*   **What is tested:** Dynamic enforcement of stricter latency SLAs.
*   **Initial State:** `v1.pvs`: Timeout 5000ms.
*   **Reload Sequence:** Publish `v2.pvs`: Timeout 100ms.
*   **Traffic Pattern:** Request targeting `/delay?ms=200` (Mock upstream sleeps 200ms).
*   **Assertions:**
    1. Pre-reload: `200 OK`.
    2. Post-reload: `504 Gateway Timeout`.
*   **Invariants:** C (Atomic Switch).
*   **Notes:** Planned / TODO – blocked on implementation.

#### `resilience_02_retry_policy_enable`
*   **Category:** Traffic Behavior Under Reload
*   **What is tested:** Activating retries for a failing upstream.
*   **Initial State:** `v1.pvs`: No retry policy.
*   **Reload Sequence:** Publish `v2.pvs`: Retry attempts=2 on `5xx`.
*   **Traffic Pattern:** Request targeting `/flaky?fail=1` (fails once, then succeeds).
*   **Assertions:**
    1. Pre-reload: `503 Service Unavailable`.
    2. Post-reload: `200 OK` (Pavis masked the failure).
*   **Invariants:** C (Atomic Switch).
*   **Notes:** Planned / TODO – blocked on implementation.

### Zone 4: Protocol & Security

#### `security_01_tls_origination_toggle`
*   **Category:** Security & TLS
*   **What is tested:** Dynamically upgrading cleartext upstream to TLS.
*   **Initial State:** `v1.pvs`: Upstream port 8080 (Plaintext).
*   **Reload Sequence:** Publish `v2.pvs`: Upstream port 8443 (TLS) + `tls_verify: false`.
*   **Traffic Pattern:** Request to `/echo`.
*   **Assertions:**
    1. Pre-reload: Echo JSON shows `tls.enabled: false`.
    2. Post-reload: Echo JSON shows `tls.enabled: true`.
*   **Invariants:** C (Atomic Switch).

---

## 5. Explicit Non-Goals
The following scenarios are **intentionally excluded** from the Runtime Suite:
*   **Relay Fanout Performance:** Testing the ability of the relay to handle 10,000 subscribers.
*   **Complex AuthZ:** OIDC/OAuth logic (dropped from core runtime scope).
*   **Ingress/WAF:** SQLi protection or deep packet inspection.
*   **K8s Controllers:** Watching Service or Ingress resources.
*   **Artifact Generation:** Testing `pavctl` logic (assumed correct for this suite).

## 6. Implementation Principles

### Verification Strategy
*   **Behavior over Logs:** Prefer asserting HTTP status codes, response headers, and upstream echo bodies (JSON) over parsing runtime logs.
*   **Triggering Reload:** Reloads must be triggered ONLY by publishing new artifacts to `pavis-mock-relay` via the `/publish` endpoint. Do not touch the filesystem directly for reload tests.

### Isolation & Determinism
*   **Mandatory Headers:** Every request sent to Pavis MUST include:
    *   `X-Pavis-Test-Case: <case_name>`
    *   `X-Pavis-Test-Run: <unique_id>`
*   **Mock-Only:** Only `pavis-mock-relay` and `pavis-mock-upstream` are permitted.
*   **Determinism:** Use status endpoints and `/received` counts on the mock upstream instead of arbitrary `sleep`.
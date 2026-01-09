# Pavis Integrated Test Suite: Architecture & Case Design

## 1. Integrated Suite Goals

The Integrated Suite serves as the final verification layer, proving that independent components (`pavctl`, `pavis-relay`, `pavis` runtime, and `pavis-mock-upstream`) function correctly as a coherent distributed system.

Its primary goals are to:
1.  **Prove the Critical Path:** Validate that a configuration change authored by a user (`pavctl`) propagates through the control plane (`relay`) and is applied by the data plane (`runtime`) without downtime.
2.  **Verify Interface Contracts:** Confirm that the actual protocols spoken by the components (long-poll, artifact validation, HTTP proxying) are compatible in a real deployment topology.
3.  **Demonstrate Resilience:** Ensure the system recovers gracefully from bad updates or component restarts in an integrated context.

It explicitly **does not** aim to re-verify exhaustive routing logic, protocol edge cases, or relay scalability, which are covered by the Runtime and Relay suites respectively.

## 2. System Topology Under Test

The suite exercises the full production-like topology in a local environment (Binary or Docker):

1.  **Publisher (`pavctl`)**: Compiles YAML into `.pvs` artifacts and publishes them to the Relay.
2.  **Control Plane (`pavis-relay`)**: Stores artifacts and distributes them to Runtimes via HTTP Long-Poll.
3.  **Data Plane (`pavis`)**: The proxy runtime. It bootstraps from local config, connects to the Relay, and hot-reloads updates.
4.  **Backend (`pavis-mock-upstream`)**: Deterministic upstream service used to verify traffic routing and resilience behaviors.

## 3. Core Integrated Invariants

Every test case must verify one or more of these invariants:

*   **I1 (End-to-End Publish):** A valid configuration compiled by `pavctl` and published to `relay` becomes active in `pavis` within a bounded time.
*   **I2 (Hot Reload Pipeline):** The runtime successfully updates its configuration via long-poll from the relay without process restarts.
*   **I3 (Artifact Opaqueness):** The relay successfully transfers artifacts regardless of content; validation responsibility lies with `pavctl` (generation) and `pavis` (load).
*   **I4 (System LKG):** If a bad update enters the relay, the runtime rejects it and maintains traffic service using the Last-Known-Good configuration.
*   **I5 (Deployment Parity):** The integration logic holds true whether components run as native binaries or Docker containers.

## 4. Case Taxonomy & Design

The suite is intentionally small, focusing on high-value integration scenarios.

### Zone 1: Smoke & Bootstrap

#### `smoke_01_full_path_bootstrap`
*   **Purpose:** Verify the "happy path" of system startup and initial configuration distribution.
*   **Initial State:** Relay running. Runtime bootstraps from a **minimal bootstrap artifact** (defines listeners and relay URL only, no upstream routing). Upstream running.
*   **Action Sequence:**
    1.  Use `pavctl` to compile `config_v1` (routes `/` to upstream).
    2.  Use `pavctl` (or curl) to publish `config_v1` to Relay.
    3.  Runtime polls Relay and applies `config_v1` via hot-reload.
*   **Traffic Pattern:** `curl` request to Runtime listener.
*   **Assertions:**
    1.  Traffic successfully reaches Upstream (200 OK) after update.
    2.  Upstream confirms receipt via `X-Pavis-Test-Case` header.
*   **Invariants Proven:** I1, I2.
*   **Determinism:** Poll Runtime listener until 200 OK (bounded loop).

### Zone 2: End-to-End Reload

#### `reload_01_traffic_shift`
*   **Purpose:** Verify dynamic reconfiguration affects traffic without restart.
*   **Initial State:** System running with `config_v1` (routes `/echo` to `backend-v1`). Traffic flowing to v1.
*   **Action Sequence:**
    1.  Assert traffic hits v1.
    2.  Compile and Publish `config_v2` (routes `/echo` to `backend-v2`) to Relay.
    3.  Wait for propagation.
*   **Traffic Pattern:** Continuous or sampled requests to `/echo`.
*   **Assertions:**
    1.  Traffic shifts from `backend-v1` to `backend-v2` within N seconds (polling check).
    2.  Runtime PID remains constant (no restart).
*   **Invariants Proven:** I1, I2, I5.
*   **Determinism:** Poll endpoint checking for `instance_id` change.

#### `reload_02_idempotent_update`
*   **Purpose:** Ensure re-publishing an identical config does not break long-poll or traffic flow (Black-box invariant).
*   **Initial State:** System running `config_v1`.
*   **Action Sequence:**
    1.  Republish `config_v1` (or recompiled equivalent) to Relay.
    2.  Wait 2x poll interval.
*   **Traffic Pattern:** Requests continue normally.
*   **Assertions:**
    1.  Traffic remains successful (200 OK).
    2.  No traffic drops or connection resets during the "update".
*   **Invariants Proven:** I2.

### Zone 3: Failure & LKG

#### `lkg_01_relay_bad_artifact`
*   **Purpose:** Verify runtime LKG protection when Relay serves bad data.
*   **Initial State:** System running `config_v1` (Valid).
*   **Action Sequence:**
    1.  Force-publish corrupt data (random bytes) to Relay (bypassing `pavctl` validation if necessary, or using `curl`).
    2.  Relay accepts and serves corrupt data.
    3.  Runtime polls, downloads, and attempts validation.
*   **Traffic Pattern:** Continuous requests to `/echo`.
*   **Assertions:**
    1.  Traffic *continues* to flow to `backend-v1`.
    2.  Runtime does *not* crash.
*   **Invariants Proven:** I3, I4.

#### `lkg_02_semantic_rejection`
*   **Purpose:** Verify runtime rejects structurally valid but semantically invalid updates.
*   **Initial State:** System running `config_v1`.
*   **Action Sequence:**
    1.  Publish `config_v2_bad` (valid PVS structure, but contains runtime-detectable error e.g., binding to a privileged port 1 or a duplicate listener).
    2.  Runtime polls and attempts to apply.
*   **Assertions:**
    1.  Traffic stays on `v1` (LKG).
    2.  Runtime does not crash.
*   **Invariants Proven:** I4.

### Zone 4: Resilience (Planned)

#### `resilience_01_relay_restart` (Planned / TODO)
*   **Purpose:** Verify Runtime reconnects after Control Plane outage.
*   **Initial State:** System healthy.
*   **Action Sequence:**
    1.  Kill `pavis-relay`.
    2.  Verify Runtime traffic continues (LKG).
    3.  Start `pavis-relay`.
    4.  Publish `config_v2`.
    5.  Verify Runtime picks up `v2`.
*   **Invariants Proven:** I2, I4.

## 5. Explicit Non-Goals

The Integrated Suite explicitly excludes:
1.  **Pavctl CLI Ergonomics:** We use `pavctl` to generate artifacts, but we don't test help text, flag parsing, or user experience.
2.  **Complex Routing:** Testing regex vs prefix precedence is done in Runtime suite. We use simple routes to prove *change*.
3.  **Relay Storage Backends:** We use one storage mode (likely memory or file) to prove integration. Exhaustive storage testing is in Relay suite.
4.  **Performance/Load:** This is functional verification, not a benchmark.

## 6. Implementation Principles

*   **Case / Runner Boundary:**
    *   Test cases **NEVER** invoke `docker` commands directly.
    *   The test runner manages lifecycle. Cases use abstract helpers (`run_pavis`, `run_relay`).
*   **Isolation:**
    *   **Binary Mode:** Use `get_free_port` for every component.
    *   **Docker Mode:** Use separate containers/networks (managed by runner).
    *   Use unique `X-Pavis-Test-Run` headers for all requests.
    *   Use unique `storage_dir` for each relay instance.
*   **Determinism:**
    *   Wait for ports to open (`wait_for_url /health`).
    *   Wait for traffic shifts by polling results, not `sleep`.
*   **Verification:**
    *   Assertions must be based on observable HTTP behavior (status codes, headers, body JSON).
    *   Logs are for debugging, not assertions (unless no other signal exists).
# Pavis Relay Test Suite: Architecture & Case Design

## 1. Relay Suite Goals

The Relay Suite validates the **Control Plane** correctness of the `pavis-relay` binary. It treats the relay as a black-box HTTP artifact distribution engine. Its primary goal is to prove that the relay:
1.  **Accepts** opaque configuration artifacts via a publication API.
2.  **Distributes** these artifacts to subscribers using efficient Long-Polling semantics (blocking until update).
3.  **Scales** (functionally) to support fanout to multiple subscribers without data loss.
4.  **Persists** state across process restarts (when configured), verifying the "Frozen Data Plane" reliability.

This suite strictly excludes the `pavis` runtime proxy and upstream services. It focuses solely on the mechanism of artifact movement and synchronization.

## 2. Core Relay Invariants

Every test case must verify one or more of these invariants:

*   **R1 (Opaque Distribution):** The relay stores and serves artifacts byte-for-byte identical to what was published, without attempting to parse or validate their content.
*   **R2 (Versioned Delivery):** Every artifact is associated with a unique **ETag** (and monotonic internal version) that subscribers use to order updates.
*   **R3 (Efficient Long-Poll):** Subscribers requesting the *current* version block (wait) until a *new* version is available or a timeout occurs.
*   **R4 (Fanout Correctness):** A single publication event propagates to ALL active long-polling subscribers eventually.
*   **R5 (Concurrency Safety):** Simultaneous publish and subscribe operations do not result in corrupted state or race conditions.
*   **R6 (Persistence):** If persistence is enabled, a restarted relay serves the Last-Known-Good (LKG) artifact immediately upon startup.
*   **R7 (Backpressure/Limits):** The relay rejects payloads exceeding configured size limits to protect resources.

## 3. Relay API Surface (Test Assumption)

The suite interacts with `pavis-relay` via standard HTTP/1.1. **ETag** is the primary change detection mechanism.

| Operation | Method / Endpoint | Critical Parameters | Expected Behavior |
| :--- | :--- | :--- | :--- |
| **Publish** | `POST /v1/publish` | Body: Raw Bytes | **200 OK**: Artifact accepted. Returns new `ETag`. |
| **Subscribe** | `GET /v1/config` | Header: `If-None-Match: <etag>`<br>Query: `timeout=<ms>` | **200 OK**: New artifact (Body + `ETag`).<br>**304 Not Modified**: No change within timeout. |
| **Health** | `GET /health` | None | **200 OK**: Ready to serve. |

## 4. Case Taxonomy

The suite is divided into six categories:

1.  **Contract & Integrity**: Basic read/write validation.
2.  **Long-Poll Semantics**: Verification of blocking and timeouts.
3.  **Fanout & Scale**: One-to-Many delivery.
4.  **Concurrency**: Race condition stress.
5.  **Persistence**: Restart recovery (Stateful).
6.  **Limits & Robustness**: Negative testing.

## 5. Detailed Case Design

### Zone 1: Contract & Integrity

#### `contract_01_opaque_publish_subscribe`
*   **Category:** Contract & Integrity
*   **What is tested:** Basic create-read cycle.
*   **Initial State:** Clean relay (isolated instance).
*   **Action:**
    1.  Publisher posts random bytes (non-valid PVS, e.g., "created-by-test").
    2.  Publisher receives 200 OK + `ETag: "A"`.
    3.  Subscriber GETs `/v1/config` (no headers).
*   **Assertions:**
    *   Subscriber receives 200 OK.
    *   Response Body == "created-by-test" (Byte-exact).
    *   Response Header `ETag` == `"A"`.
*   **Invariants Proven:** R1 (Opaque), R2 (Versioned).

#### `contract_02_idempotency_check`
*   **Category:** Contract & Integrity
*   **What is tested:** Republishing identical bytes does not break subscribe semantics.
*   **Initial State:** Relay serving "payload-v1" (`ETag: "A"`).
*   **Action:**
    1.  Publisher posts "payload-v1" again.
    2.  Publisher receives 200 OK + `ETag: "B"` (or `"A"` depending on implementation, but must be valid).
    3.  Subscriber GETs `/v1/config`.
*   **Assertions:**
    *   Subscriber receives 200 OK.
    *   Body is "payload-v1".
    *   Response `ETag` matches the one returned in step 2.
*   **Invariants Proven:** R1 (Opaque), R5 (Concurrency Safety - implied stability).

### Zone 2: Long-Poll Semantics

#### `longpoll_01_wait_for_update`
*   **Category:** Long-Poll Semantics
*   **What is tested:** Subscriber blocks until update occurs.
*   **Initial State:** Relay serving "v1" (`ETag: "1"`).
*   **Client Setup:** Client A requests `/v1/config` with `If-None-Match: "1"` and `timeout=5000` (5s).
*   **Action:**
    1.  Client A initiates request (blocks).
    2.  *Trigger:* Publisher posts "v2".
*   **Assertions:**
    *   Client A returns successfully.
    *   Status is 200 OK (not 304).
    *   Body is "v2".
    *   Response `ETag` != `"1"`.
*   **Invariants Proven:** R3 (Efficient Long-Poll), R2 (Versioned).
*   **Determinism:** Polling for "v2" publish completion ensures Client A unblocks.

#### `longpoll_02_timeout_no_change`
*   **Category:** Long-Poll Semantics
*   **What is tested:** Subscriber waits for full timeout if no update occurs.
*   **Initial State:** Relay serving "v1" (`ETag: "1"`).
*   **Client Setup:** Client requests `/v1/config` with `If-None-Match: "1"` and `timeout=2000` (2s).
*   **Action:**
    1.  No publish occurs.
    2.  Wait for client request to complete.
*   **Assertions:**
    *   Request duration is >= 2s.
    *   Status is 304 Not Modified.
    *   Body is empty.
*   **Invariants Proven:** R3 (Efficient Long-Poll).

### Zone 3: Fanout & Scale

#### `fanout_01_multi_subscriber_broadcast`
*   **Category:** Fanout
*   **What is tested:** One publish wakes up multiple pending subscribers.
*   **Initial State:** Relay serving "v1" (`ETag: "1"`).
*   **Client Setup:** 5 separate background clients start long-poll with `If-None-Match: "1"`.
*   **Action:**
    1.  Publisher posts "v2".
    2.  Wait for all background clients to exit.
*   **Assertions:**
    *   ALL 5 clients return with 200 OK.
    *   ALL 5 clients receive "v2" body.
*   **Invariants Proven:** R4 (Fanout Correctness).

#### `fanout_02_catch_up`
*   **Category:** Fanout
*   **What is tested:** A subscriber that is behind (old ETag) gets immediate update.
*   **Initial State:** Relay serving "v5" (`ETag: "5"`).
*   **Client Setup:** Client requests `/v1/config` with `If-None-Match: "1"` (very old).
*   **Action:**
    1.  Request sent.
*   **Assertions:**
    *   Return is IMMEDIATE (no blocking).
    *   Status 200 OK.
    *   Body is "v5".
*   **Invariants Proven:** R2 (Versioned Delivery).

### Zone 4: Concurrency

#### `concurrency_01_rapid_publish`
*   **Category:** Concurrency
*   **What is tested:** Relay handles high-frequency updates without crashing.
*   **Initial State:** Clean relay.
*   **Action:**
    1.  Script loops 50 times, publishing "payload-N".
    2.  Subscriber polls continuously in parallel.
*   **Assertions:**
    *   Relay process remains alive (check `/health`).
    *   Final subscriber poll returns "payload-50".
    *   Intermediate versions may be skipped, but ETag/Version must behave monotonically/consistently.
*   **Invariants Proven:** R5 (Concurrency Safety), R2 (Versioned).

### Zone 5: Persistence (Storage Mode)

*Requires `TEST_STORAGE=file` or specific config.*

#### `persistence_01_restart_recovery`
*   **Category:** Persistence
*   **What is tested:** LKG is saved to disk and restored.
*   **Initial State:** Relay started with `--storage-dir <temp_dir>`.
*   **Action:**
    1.  Publish "payload-persistent".
    2.  Verify GET returns "payload-persistent".
    3.  **Kill** relay process.
    4.  **Start** new relay process pointing to same `<temp_dir>`.
    5.  Immediate GET `/v1/config` (no headers).
*   **Assertions:**
    *   Request returns 200 OK immediately.
    *   Body is "payload-persistent".
*   **Invariants Proven:** R6 (Persistence).

### Zone 6: Limits & Robustness

#### `limits_01_oversized_payload`
*   **Category:** Limits
*   **What is tested:** Relay rejects huge bodies.
*   **Initial State:** Relay configured with `max_body_size = 1MB` (or known default).
*   **Action:**
    1.  Publish 5MB of zero bytes.
*   **Assertions:**
    *   Status is 413 Payload Too Large (or 400 Bad Request).
    *   Subsequent GET returns previous valid version (relay state unchanged).
*   **Invariants Proven:** R7 (Backpressure/Limits).

#### `limits_02_empty_publish`
*   **Category:** Limits
*   **What is tested:** Handling of empty body.
*   **Action:**
    1.  Publish empty body (0 bytes).
*   **Assertions:**
    *   Should behave deterministically (either accept as valid empty config, or reject 400).
    *   If accepted, subscribers get 0 bytes.
*   **Invariants Proven:** R1 (Opaque).

#### `robustness_01_subscriber_reconnect`
*   **Category:** Robustness
*   **What is tested:** Subscriber disconnects and reconnects with old ETag.
*   **Initial State:** Relay serving "v2" (`ETag: "2"`).
*   **Action:**
    1.  Client starts long-poll with `If-None-Match: "2"` (blocks).
    2.  Client aborts connection (simulated disconnect).
    3.  Publisher posts "v3".
    4.  Client reconnects with `If-None-Match: "2"`.
*   **Assertions:**
    *   Reconnect returns IMMEDIATE 200 OK.
    *   Body is "v3".
*   **Invariants Proven:** R2 (Versioned Delivery), R3 (Efficient Long-Poll).

## 6. Implementation Principles

### Isolation & Setup
*   **Isolated Instance:** Each test case MUST run against a fresh, isolated `pavis-relay` instance.
*   **Ports:**
    *   **Binary Mode:** Use `get_free_port` for dynamic allocation.
    *   **Docker Mode:** Use fixed ports mapped to dynamic host ports or internal network addressing.
    *   **Constraint:** Test logic must accept the target URL as an argument/variable.
*   **Storage:** Use unique temporary directories for persistence tests; ensure cleanup.

### Determinism Strategies
*   **Wait for Port:** Always poll `/health` or TCP port open before starting test logic.
*   **No Fixed Sleeps:** Instead of `sleep 1`, use `curl --max-time` or a polling loop checking for status changes.
*   **Background Jobs:** Use `&` for subscribers, `wait` for completion.

### Explicit Non-Goals
*   **Validation:** We do NOT validate that the payload is a valid FlatBuffer/JSON. The relay is a dumb pipe.
*   **Runtime Config:** We do not check if `pavis` runtime likes the config.
*   **AuthZ:** Token validation is out of scope for this suite (unless auth feature is added later).
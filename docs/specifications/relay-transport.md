# Pavis Config Serving & Long-Polling Specification

**Version:** 1.0
**Status:** Final
**Date:** 2026-01-17

---

## 1. Introduction

This document specifies the HTTP-based config serving and long-polling mechanism for Pavis relay servers. It defines how clients fetch immutable configuration artifacts and efficiently detect changes using content-addressed identity.

**Design Choice: HTTP + Long-Poll**

This specification uses HTTP with long-polling rather than push-based protocols (WebSockets, gRPC streaming, SSE) as a deliberate Frozen Data Plane design choice. HTTP conditional requests provide:
- Universal client compatibility (any HTTP library)
- Natural retry semantics and idempotency
- Transparent proxy/CDN integration
- Simpler operational debugging (curl, standard tooling)

Long-polling balances efficiency with simplicity, avoiding the connection management complexity of bidirectional streaming.

### 1.1 Scope

This specification covers:
- Resource model for `/v1/config`
- HTTP semantics for fetching and change detection
- Long-polling behavior for efficient updates
- Client responsibilities and server guarantees

This specification does NOT cover:
- History browsing or artifact retrieval by version number
- Partial config streaming or semantic diffing
- Publish API semantics
- Delivery latency guarantees

---

## 2. Design Principles

### 2.1 Artifact Immutability

Configuration artifacts (PVS binaries) are immutable. Once published, their byte content MUST NOT change. Re-publishing identical bytes is permitted and MUST result in the same identity.

### 2.2 Content-Addressed Identity

Artifact identity is defined exclusively by its byte content, not by metadata. The authoritative identity mechanism is the **checksum** (SHA-256 of PVS bytes). All change detection, caching, and deduplication MUST be based on checksum.

### 2.3 Version Numbers as Observability Metadata

Version numbers are monotonically increasing counters used solely for:
- Human-readable observability (logs, metrics, debugging)
- Ordering events in time-series data

Version numbers MUST NOT be used for:
- Determining artifact identity
- Change detection or cache invalidation
- Client-side deduplication logic

**Rationale:** Content identity and temporal ordering are orthogonal concerns. Separating them prevents ambiguity when identical artifacts are re-published at different times.

---

## 3. Resource Model

### 3.1 Resource Definition

The resource `/v1/config` represents the **current configuration artifact** published to the relay.

**Properties:**
- **Current**: Always reflects the most recently published artifact
- **Singular**: Only one current config exists at any time
- **Immutable**: The resource content changes atomically when a new artifact is published
- **Always Available**: The resource MUST return a valid artifact once the relay is ready (see Section 5.6)

### 3.2 Resource Identity

The identity of the resource is the **ETag**, which MUST be derived from the checksum of the PVS bytes.

**ETag Format:**
```
ETag: "sha256:<hex-digest>"
```

Example:
```
ETag: "sha256:a3c5b4..."
```

The ETag is a **strong validator** as defined in RFC 9110 Section 8.8.1, indicating byte-for-byte equivalence.

---

## 4. ETag Semantics

### 4.1 ETag as Strong Validator

The ETag MUST be a strong entity tag. Two resources with the same ETag are guaranteed to have identical byte content.

### 4.2 ETag Derivation

```
ETag = "sha256:" + hex(sha256(PVS_bytes))
```

The checksum MUST be computed over the exact bytes served in the response body.

### 4.3 Why Not Version Numbers?

Version numbers are unsuitable for identity because:
1. **Re-publication:** Identical artifacts may receive different version numbers if re-published
2. **Ambiguity:** Version N does not identify a unique artifact across its lifecycle
3. **Semantic Coupling:** Version implies ordering semantics irrelevant to content identity

**Correct Usage:**
- **Identity:** Use ETag/checksum
- **Ordering/Observability:** Use version number (in `x-config-version` header)

---

## 5. Request Semantics

### 5.1 Basic GET Request

**Request:**
```http
GET /v1/config HTTP/1.1
Host: relay.example.com
```

**Behavior:**
- The server MUST return the current configuration artifact immediately
- No waiting or blocking occurs

**Response:** See Section 6.1

### 5.2 Long-Poll Request

**Request:**
```http
GET /v1/config?wait_ms=30000 HTTP/1.1
Host: relay.example.com
If-None-Match: "sha256:abc123..."
```

**Parameters:**
- `wait_ms` (integer, optional): Maximum wait time in milliseconds
  - Range: 1 to 60000 (1ms to 60 seconds)
  - If omitted or 0: no waiting (immediate response)
  - If out of range: server MUST return `400 Bad Request`

**Behavior:**
1. Server compares client's `If-None-Match` value with current ETag
2. If different: return immediately with `200 OK` and new config
3. If same:
   - If `wait_ms` > 0: hold request until change or timeout
   - If `wait_ms` omitted/0: return immediately with `304 Not Modified`
4. If change occurs during wait: return `200 OK` with new config
5. If timeout occurs: return `204 No Content` (see Section 6.2)

### 5.3 Conditional Request (No Long-Poll)

**Request:**
```http
GET /v1/config HTTP/1.1
Host: relay.example.com
If-None-Match: "sha256:abc123..."
```

**Behavior:**
- Compare current ETag with `If-None-Match`
- If same: return `304 Not Modified` (no body)
- If different: return `200 OK` with config

### 5.4 If-None-Match Header

Clients MAY include the `If-None-Match` header to perform conditional requests. The value MUST be a quoted ETag string.

**Valid:**
```
If-None-Match: "sha256:abc123..."
```

**Invalid:**
```
If-None-Match: sha256:abc123...    (missing quotes)
If-None-Match: "abc123..."          (missing prefix)
```

If the header is malformed, the server SHOULD ignore it and treat the request as unconditional.

### 5.5 Request Without If-None-Match

If `If-None-Match` is absent:
- Basic GET: return `200 OK` with current config
- Long-poll (with `wait_ms`): Undefined behavior (see Section 5.7)

### 5.6 Unready State

If the relay has not yet published any config (startup state), the server SHOULD return:
```
503 Service Unavailable
Retry-After: 1
```

Once a config is published, the relay MUST respond with `200 OK` or `304 Not Modified` as appropriate.

### 5.7 Long-Poll Without If-None-Match

The semantics of `wait_ms` without `If-None-Match` are ambiguous (cannot determine "no change" without a reference point).

**Recommended Behavior:**

Implementations SHOULD treat this as an unconditional GET and return immediately with `200 OK`, ignoring `wait_ms`.

**Rationale:** Long-polling requires a baseline for change detection. Without `If-None-Match`, there is no reference point, so waiting is semantically meaningless.

**Alternative Behaviors (Permitted but Discouraged):**
- Return `400 Bad Request` to enforce strict usage
- Wait for next publish event and return `200 OK` (any artifact, even if unchanged)

Implementations choosing alternative behavior MUST document it clearly.

---

## 6. Response Semantics

### 6.1 200 OK - Config Available

**When:**
- Unconditional GET
- Conditional GET where ETag differs
- Long-poll completed with change

**Headers (Required):**
```http
HTTP/1.1 200 OK
Content-Type: application/octet-stream
ETag: "sha256:abc123..."
x-config-size: 1234
Cache-Control: no-store
```

**Headers (Optional):**
```http
x-config-version: 42
x-config-generated-at: 2026-01-17T10:30:00Z
```

**Body:**
- MUST contain the full PVS binary artifact
- Length MUST match `x-config-size`

### 6.2 304 Not Modified - No Change (Conditional GET)

**When:**
- Conditional GET where `If-None-Match` matches current ETag (without `wait_ms` or `wait_ms=0`)

**Headers (Required):**
```http
HTTP/1.1 304 Not Modified
Cache-Control: no-store
```

**Headers (SHOULD Include):**
```http
ETag: "sha256:abc123..."
```

**Body:**
- MUST be empty (no content)

**Note:** The server SHOULD include the `ETag` header for consistency with RFC 9110, though it is not strictly required for 304 responses.

### 6.3 204 No Content - Long-Poll Timeout

**When:**
- Long-poll request (`wait_ms` > 0) with `If-None-Match` matching current ETag
- Timeout occurs with no change

**Headers (Required):**
```http
HTTP/1.1 204 No Content
ETag: "sha256:abc123..."
Cache-Control: no-store
```

**Body:**
- MUST be empty (no content)

**Rationale:** 204 No Content distinguishes long-poll timeout from conditional GET cache validation. Clients can treat 204 as "retry immediately" without ambiguity.

### 6.4 400 Bad Request - Invalid Parameters

**When:**
- `wait_ms` out of range (< 1 or > 60000)
- Malformed query parameters

**Body:**
- SHOULD include human-readable error message

### 6.5 503 Service Unavailable - Not Ready

**When:**
- Relay has not yet received a config artifact

**Headers:**
```http
HTTP/1.1 503 Service Unavailable
Retry-After: 1
```

**Body:**
- MUST be empty (no content)

### 6.6 Header Definitions

#### 6.6.1 ETag (Required in 200/304/204)

Strong entity tag identifying the artifact by checksum.

**Format:** `"sha256:<hex>"`

**Presence Requirements:**
- **200 OK**: MUST be included
- **304 Not Modified**: SHOULD be included
- **204 No Content**: MUST be included

#### 6.6.2 x-config-size (Required in 200)

Size of the PVS artifact in bytes.

**Format:** Integer as string

**Purpose:**
- **Transport Integrity:** Allows clients to verify the received body matches the expected length
- **Buffer Pre-allocation:** Enables efficient memory management
- **Corruption Detection:** Early detection of truncated or malformed responses

**Client Requirement:** Clients SHOULD verify that the received body length matches `x-config-size`. Mismatch indicates transport corruption or attack.

#### 6.6.3 x-config-version (Optional, Observability Only)

Monotonic version counter assigned by the relay at publish time.

**Format:** Integer as string

**Purpose:** Debugging, logging, and metrics. MUST NOT be used for change detection.

**Warning:** Different version numbers may have identical ETags (re-publication case).

#### 6.6.4 x-config-generated-at (Optional)

Timestamp when the artifact was published to the relay.

**Format:** ISO 8601 (RFC 3339)

**Purpose:** Observability and debugging.

---

## 7. Long-Poll Behavior

### 7.1 Server-Side Waiting

When a long-poll request is received (valid `wait_ms` and `If-None-Match` matches):
1. The server MUST hold the HTTP connection open
2. The server MUST monitor for config changes (new publish events)
3. The server MUST enforce the `wait_ms` timeout

### 7.2 Wake-Up Conditions

The server MUST terminate the wait and respond when:
1. **Change Detected:** A new artifact is published (different ETag)
   - Response: `200 OK` with new config
2. **Timeout Reached:** `wait_ms` milliseconds elapse with no change
   - Response: `204 No Content`
3. **Connection Lost:** Client disconnects or network error
   - No response needed (connection closed)

### 7.3 Immediate Response Condition

If the current ETag differs from `If-None-Match`, the server MUST respond immediately with `200 OK` (no waiting), even if `wait_ms` is specified.

### 7.4 Idempotency and Retry Safety

Long-poll requests are idempotent and safe (no side effects). Clients MAY retry freely.

**Recommended Client Retry Logic:**
- On `200 OK`: Process config, then retry with new ETag
- On `204 No Content`: Long-poll timeout, retry immediately with same ETag
- On `304 Not Modified`: Conditional GET cache hit, retry immediately with same ETag
- On `503 Service Unavailable`: Retry after `Retry-After` duration
- On network error: Retry with exponential backoff

---

## 8. Client Responsibilities

### 8.1 State Persistence

Clients MUST persist the current ETag locally to enable change detection across restarts.

**Minimum Required State:**
```
current_etag: String    # "sha256:abc123..."
```

**Recommended Additional State:**
```
current_version: u64    # For logging/debugging
config_bytes: Vec<u8>   # The PVS artifact
```

### 8.2 Change Detection

Clients MUST determine if a config has changed by comparing ETags:

```
if response.etag != local_state.current_etag {
    // Config changed, process new artifact
    process_config(response.body);
    local_state.current_etag = response.etag;
} else {
    // No change, ignore
}
```

### 8.3 Deduplication

Clients MUST NOT re-process identical artifacts. ETag comparison provides exact deduplication.

**Anti-Pattern (DO NOT DO):**
```
if response.version > local_state.current_version {
    // WRONG: Version may increment even if content is identical
}
```

### 8.4 Version Number Usage

Clients SHOULD treat `x-config-version` as **observability metadata only**:
- Include in logs and metrics for correlation
- Display in debugging UIs
- MUST NOT use for change detection or cache validation

### 8.5 Error Handling

Clients MUST handle:
- **503 Service Unavailable:** Retry with `Retry-After` delay
- **Network Errors:** Retry with exponential backoff
- **400 Bad Request:** Fix request parameters, do not retry unchanged

### 8.6 Transport Integrity Validation

Clients SHOULD validate transport integrity on every successful response:

**Required Checks:**
1. Verify `Content-Length` or actual body length matches `x-config-size`
2. Verify `sha256(response_body)` matches ETag checksum

**Example (Pseudocode):**
```
let body_bytes = response.read_body();
assert(body_bytes.len() == response.header("x-config-size").parse::<usize>());
assert(sha256_hex(body_bytes) == response.header("ETag").strip_prefix("sha256:"));
```

**Rationale:** These checks detect:
- Transport corruption (truncation, modification)
- MITM attacks
- Server implementation bugs
- Proxy interference

---

## 9. Non-Goals

The following are explicitly **out of scope** for this specification:

### 9.1 History Browsing

No API is defined for fetching historical configs by version number or timestamp. The `/v1/config` endpoint serves only the **current** artifact.

**Rationale:** History is an operational/debugging concern, not a runtime concern. If needed, implement separately (e.g., `/v1/history/{version}`).

### 9.2 Partial Config Streaming

The full PVS artifact is always returned. No support for range requests, chunking, or partial updates.

**Rationale:** PVS artifacts are cryptographically validated as atomic units. Partial delivery breaks integrity checks.

### 9.3 Semantic Diffing

No support for semantic diffs or incremental patches. Clients receive the full artifact and determine applicability internally.

**Rationale:** Config interpretation is client-specific. The relay is protocol-agnostic.

### 9.4 Delivery Latency Guarantees

This specification does NOT define maximum latency from publish to client fetch. Long-poll provides efficient detection but does NOT guarantee push notification latency.

---

## 10. Examples

### 10.1 First Fetch (Cold Start)

**Client has no prior state**

```http
GET /v1/config HTTP/1.1
Host: relay.example.com
```

**Response:**
```http
HTTP/1.1 200 OK
Content-Type: application/octet-stream
ETag: "sha256:abc123..."
x-config-size: 1234
x-config-version: 1
x-config-generated-at: 2026-01-17T10:00:00Z

[PVS binary data, 1234 bytes]
```

**Client Action:**
- Store ETag: `"sha256:abc123..."`
- Store version: `1` (for logging)
- Process config

---

### 10.2 Long-Poll with No Change

**Client has ETag from previous fetch**

```http
GET /v1/config?wait_ms=30000 HTTP/1.1
Host: relay.example.com
If-None-Match: "sha256:abc123..."
```

**Server Behavior:**
- Compare: current ETag = `"sha256:abc123..."` (matches)
- Wait up to 30 seconds for change
- Timeout occurs (no new publish)

**Response:**
```http
HTTP/1.1 204 No Content
ETag: "sha256:abc123..."
Cache-Control: no-store
```

**Client Action:**
- Long-poll timeout, no change detected
- Retry immediately with same ETag

---

### 10.3 Long-Poll with Change

**Client has ETag from previous fetch**

```http
GET /v1/config?wait_ms=30000 HTTP/1.1
Host: relay.example.com
If-None-Match: "sha256:abc123..."
```

**Server Behavior:**
- Compare: current ETag = `"sha256:abc123..."` (matches)
- Wait for change
- After 5 seconds, new config published with ETag `"sha256:def456..."`
- Immediately respond

**Response:**
```http
HTTP/1.1 200 OK
Content-Type: application/octet-stream
ETag: "sha256:def456..."
x-config-size: 1456
x-config-version: 2

[New PVS binary data, 1456 bytes]
```

**Client Action:**
- Compare: `"sha256:def456..."` ≠ `"sha256:abc123..."` (changed)
- Update local ETag to `"sha256:def456..."`
- Update local version to `2` (for logging)
- Process new config

---

### 10.4 Re-Publication of Identical Artifact

**Timeline:**
1. Version 1 published: ETag `"sha256:aaa111..."`, version `1`
2. Version 2 published (different content): ETag `"sha256:bbb222..."`, version `2`
3. Version 3 published (same as version 1): ETag `"sha256:aaa111..."`, version `3`

**Client Behavior:**
- At version 1: stores ETag `"sha256:aaa111..."`
- At version 2: detects change (ETag differs), processes new config, stores ETag `"sha256:bbb222..."`
- At version 3: detects change (ETag differs back to `"sha256:aaa111..."`), but content is identical to version 1
  - Client MAY optimize by checking if this ETag was previously seen
  - Client MUST NOT assume version 3 > version 1 means content is newer

**Server Response for Client at Version 2:**

```http
GET /v1/config HTTP/1.1
If-None-Match: "sha256:bbb222..."
```

```http
HTTP/1.1 200 OK
ETag: "sha256:aaa111..."
x-config-version: 3

[PVS bytes identical to version 1]
```

**Key Point:** Version `3` > version `2`, but content is identical to version `1`. Version numbers do NOT indicate "newer" content.

---

## 11. Security Considerations

### 11.1 ETag Integrity

The ETag MUST be derived from the actual response body bytes. Clients SHOULD verify:
```
sha256(response.body) == etag.strip_prefix("sha256:")
```

Mismatched ETag indicates corruption or attack (MITM).

### 11.2 Long-Poll Resource Exhaustion

Servers SHOULD implement:
- Per-client connection limits
- Maximum concurrent long-poll requests
- Aggressive timeouts for misbehaving clients

### 11.3 Cache Directives

The `Cache-Control: no-store` header MUST be included in all responses to prevent intermediate proxies from caching artifacts without ETag awareness.

**Rationale:**
- **Correctness:** ETag-based validation must occur at the origin server, not at intermediary caches
- **Security:** Prevents stale configs from being served by CDNs or corporate proxies
- **Simplicity:** Eliminates cache coherence complexity; clients control caching via ETag persistence

Without `no-store`, proxies might:
- Serve stale artifacts to clients with correct ETags
- Bypass long-poll semantics
- Break transport integrity validation

---

## 12. Conformance

### 12.1 MUST Requirements

Implementations MUST:
- Use SHA-256 checksum as ETag
- Return `200 OK` with body for new config
- Return `304 Not Modified` without body for conditional GET with matching ETag (no long-poll)
- Return `204 No Content` without body for long-poll timeout with no change
- Enforce `wait_ms` range (1-60000)
- Include required headers:
  - `ETag` in 200 OK and 204 No Content responses
  - `x-config-size` in 200 OK responses
  - `Cache-Control: no-store` in all successful responses
- Ensure all error responses (400, 503) and success responses without body (204, 304) have empty bodies

### 12.2 SHOULD Requirements

Implementations SHOULD:
- Return `503 Service Unavailable` when not ready
- Include `ETag` in 304 Not Modified responses
- Include `x-config-version` for observability
- Validate client-provided ETags
- Treat long-poll without `If-None-Match` as unconditional GET (return `200 OK` immediately)

### 12.3 MAY Requirements

Implementations MAY:
- Include `x-config-generated-at` timestamp
- Define behavior for long-poll without `If-None-Match`

---

**End of Specification**

# Relay Distribution Protocol Specification

## 1) Overview

This specification defines the HTTP-based distribution protocol used by a Relay to publish and serve configuration artifacts (`.pvs`) to Pavis runtimes. The protocol uses long-polling to efficiently notify subscribers of new artifacts while preserving simple, universally compatible semantics. The Relay is authoritative for version assignment, while artifact identity is content-addressed via SHA-256 checksums represented as HTTP ETags.

The protocol is designed for a frozen data plane: runtimes do not interpret, compile, or validate configuration beyond integrity checks; they only execute fully materialized artifacts supplied by the Relay.

## 2) Goals and Non-Goals

### Goals
- Distribute configuration artifacts with low latency and minimal overhead.
- Provide a robust, deterministic identity model based on artifact bytes.
- Maintain monotonic, relay-assigned versions for ordering and observability.
- Support large fan-out via efficient long-polling.
- Preserve crash-safe, persistent Last Known Good (LKG) behavior.
- Provide explicit resynchronization semantics when conditional evaluation is unreliable.

### Non-Goals
- Rollback API for historical versions.
- Multi-tenancy or multiple independent configuration streams per Relay instance.
- Partial or incremental configuration updates.
- Push streaming protocols (WebSocket, SSE, gRPC streaming).

## 3) Core Invariants

1. Frozen Data Plane: Runtimes MUST NOT interpret, compile, or apply defaults to configuration. Only fully materialized artifacts are executed.
2. Artifact identity is defined solely by checksum (ETag).
3. Version is ordinal only and MUST NOT be used for deduplication.
4. Relay MUST always be able to serve at least the LKG unless storage is broken.
5. 204 and 304 are both NoUpdate and MUST NOT trigger backoff.
6. 404 MUST NOT be used for configuration fetch.
7. The protocol MUST include an explicit resynchronization signal.
8. Upon resync, the client MUST reset conditional state and immediately refetch unconditionally.
9. Need-Resync MUST NOT be backoff-gated.
10. Runtime MUST NOT apply any artifact that fails verification.
11. Runtime MUST NOT re-apply an artifact with the same checksum.

## 4) Artifact Identity and Versioning

- Artifact identity is the SHA-256 checksum of the artifact bytes.
- The canonical ETag format is exactly: "sha256:<lowercase-hex>".
- Artifact identity is defined solely by checksum (ETag).
- Version is ordinal only and MUST NOT be used for identity or deduplication.
- Multiple versions MAY legally refer to the same checksum.
- The Relay assigns versions that are strictly monotonic.
- Version increments occur only on successful publish and persistence of the new LKG.

## 5) Last Known Good (LKG) Semantics

- The Relay MUST maintain exactly one persistent LKG artifact.
- LKG updates MUST be atomic (write temp + fsync + rename).
- Relay crash during publish MUST leave either the old or the new LKG, never partial state.
- Relay MUST always be able to serve at least the LKG unless storage is broken.
- LKG metadata MUST include: version, checksum, size, and published timestamp.

## 6) Fetch Endpoint Definition

### Endpoint
```
GET /v1/config?wait_ms=<ms>
```

### Request Format
- HTTP GET with optional `wait_ms` query parameter.
- Conditional requests use `If-None-Match` with exactly one strong ETag.
- Clients MUST NOT issue concurrent fetch requests to the Fetch Endpoint.

### Headers
- `If-None-Match` (optional): MUST be a single strong ETag of the form "sha256:<lowercase-hex>".
  - Weak ETags (W/...), wildcard (`*`), multiple ETags, non-sha256 prefixes, or incorrect lengths MUST be treated as invalid.
  - Invalid or missing `If-None-Match` MUST be treated as an unconditional GET.

### Query Parameters
- `wait_ms` (optional, integer):
  - Range: 0..=60000 (values above range MUST be rejected with 400).
  - `wait_ms=0` means no long-poll; response is immediate.
  - `wait_ms>0` allows long-polling if the conditional ETag matches.

## 7) Response Semantics

All responses produced by the Relay MUST fall into exactly one of these four semantic classes: NewArtifact, NoUpdate, TransientUnavailable, NeedResync. Each HTTP status code used by the protocol MUST map unambiguously to exactly one of these classes. Clients MUST implement all behavior exclusively in terms of these four semantic classes and MUST NOT infer semantics from raw HTTP status codes beyond this mapping.

### 200 OK (NewArtifact)
- Returned when:
  - Unconditional GET, or
  - Conditional ETag does not match current artifact.
- Body: artifact bytes.
- Headers:
  - `ETag`: strong ETag ("sha256:<lowercase-hex>")
  - `x-config-version`: relay-assigned version (ordinal only)
  - `x-config-size`: artifact size in bytes
  - `Cache-Control: no-store`

### 204 No Content (NoUpdate)
- Returned when:
  - Conditional ETag matches and long-poll times out (`wait_ms > 0`).
- Body: empty.
- Headers:
  - `ETag`: current artifact ETag
  - `Cache-Control: no-store`

### 304 Not Modified (NoUpdate)
- Returned when:
  - Conditional ETag matches and `wait_ms=0` (immediate no-update).
- Body: empty.
- Headers:
  - `ETag`: current artifact ETag
  - `Cache-Control: no-store`

### 5xx Errors (TransientUnavailable)
- Returned only when the Relay cannot serve any artifact at all (including the LKG).
- Body MAY be empty or a diagnostic string.

### 410 Gone (NeedResync)
- 410 Gone MUST be used to signal that the Relay cannot reliably evaluate conditional requests against the current identity (integrity metadata unavailable, identity cache desynchronized, storage corruption).
- The Relay MUST return 410 Gone only when it cannot safely evaluate conditional requests but can still serve an unconditional baseline artifact.
- Need-Resync is a distinct semantic class and MUST NOT be treated as an error.

### LKG / 200 vs 410 vs 5xx Boundary
- If the Relay has a durable LKG artifact available and its checksum can be determined, it MUST return that artifact with 200 OK on an unconditional GET, even if internal conditional-evaluation state is unavailable, corrupted, or desynchronized.
- The Relay MUST return 410 Gone only when it cannot safely evaluate conditional requests but can still serve an unconditional baseline artifact.
- The Relay MUST return 5xx only when it cannot serve any artifact at all (including the LKG).

## 8) Long-Polling Behavior

- If `If-None-Match` is valid and matches the current ETag:
  - If `wait_ms > 0`: register a waiter and block until notify or timeout.
  - If `wait_ms = 0`: respond immediately with 304.
- When a publish results in a new checksum, the Relay MUST notify all waiters.
- Wake-ups MUST re-check the current ETag; if unchanged, the waiter MUST continue to wait until timeout.
- ETag comparison MUST be a byte-for-byte string equality comparison.

## 9) Publish Semantics

### Validation
- Relay MUST verify `.pvs` artifact integrity before acceptance.
- Any invalid or corrupt artifact MUST be rejected; version MUST NOT advance.

### Version Assignment
- Relay MUST assign `new_version = current_version + 1` on successful publish.
- Version increments MUST be serialized to preserve monotonicity.

### LKG Persistence
- Relay MUST persist the new artifact as the LKG before acknowledging publish success.
- LKG update MUST be atomic and crash-safe.

### Waiter Notification
- On publish with a new checksum, Relay MUST notify all long-poll waiters.
- On publish with an identical checksum, the Relay MUST NOT notify long-poll waiters.

## 10) Deduplication Semantics

- Artifact identity is defined solely by checksum (ETag).
- Runtime MUST NOT re-apply an artifact with the same checksum.
- Version MUST NOT be used for deduplication.

## 11) Resynchronization Semantics

### When Relay MUST Signal Resync
The Relay MUST respond with 410 Gone if it cannot reliably evaluate `If-None-Match` against the current identity, including (but not limited to):
- Integrity metadata unavailable or corrupt.
- Persistent storage failure that prevents determining current identity.
- Internal state desynchronization that makes conditional evaluation unsafe.

### Client Obligations on Resync
Upon receiving Need-Resync (410):
- Client MUST discard all conditional state (current ETag, rejected ETags).
- Client MUST reset all retry/backoff state.
- Client MUST immediately perform an unconditional GET (no `If-None-Match`).
- Need-Resync MUST NOT be backoff-gated.

## 12) Error Handling Matrix

| Response Class | HTTP Status | Meaning | Client Action |
|---------------|-------------|---------|---------------|
| NewArtifact | 200 | New artifact available | Validate, deduplicate, apply if new |
| NoUpdate | 204 | No update after long-poll | Immediate retry (no backoff) |
| NoUpdate | 304 | No update on immediate check | Immediate retry (no backoff) |
| TransientUnavailable | 5xx | Relay unavailable | Exponential backoff |
| NeedResync | 410 | Conditional state unreliable | Reset state, immediate unconditional fetch |

## 13) Backoff and Retry Requirements

- 204 and 304 are both NoUpdate and MUST NOT trigger backoff.
- 5xx and network errors SHOULD trigger exponential backoff.
- Need-Resync MUST NOT be backoff-gated.
- Unconditional retry is REQUIRED after 410.

## 14) Concurrency Model

- Publish operations MUST be serialized to preserve monotonic versions.
- Fetch operations MUST be fully concurrent.
- Waiter notification MUST be fan-out to all registered long-poll clients.

## 15) Persistence and Crash Safety

- LKG persistence MUST be atomic and crash-safe.
- History MAY be maintained, but LKG is mandatory.
- On restart, Relay MUST restore state from LKG.
- Relay MUST continue serving the last durable LKG unless storage is broken.

## 16) Explicitly Unsupported Features

- Rollback API
- Multi-tenancy
- Partial/differential configuration updates
- Push streaming protocols (WebSocket, SSE)

## 17) Security and Integrity Considerations

- All artifacts MUST be integrity-verified before acceptance.
- Verification MUST include checksum verification against the ETag, artifact format validation, and schema/version compatibility validation.
- Runtime MUST NOT apply any artifact that fails verification.
- ETag must reflect SHA-256 of artifact bytes; any deviation is a protocol violation.
- `Cache-Control: no-store` MUST be used to prevent intermediary caching.

## 18) Compatibility and Extensibility Notes

- Headers are extensible; new headers MAY be added without breaking clients.
- Clients MUST ignore unknown headers.
- This specification is self-contained and authoritative for Relay distribution behavior.

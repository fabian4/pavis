# Relay Distribution Protocol Specification

> **Class:** SPECIFICATION  
> **Question:** How does the Relay distribute configuration updates efficiently?  
> **Authority:** This document is normative. Implementation resides in code (`crates/pavis-relay`).

---

## Overview

The Pavis Relay implements an efficient configuration distribution protocol using **HTTP long-polling** to minimize latency and network overhead while maintaining simplicity and universal client compatibility.

**Design Choice:** HTTP + long-polling over WebSockets/gRPC streaming/SSE for:
- Universal client compatibility (any HTTP library)
- Natural retry semantics and idempotency
- Transparent proxy/CDN integration
- Simpler operational debugging

---

## Version Monotonicity

Versions are strictly increasing integers assigned by the relay:

### Rules

1. **Strictly Monotonic**: `new_version = current_version + 1`
2. **Relay-Generated**: Clients cannot specify versions
3. **Atomic**: Version increment and artifact storage are atomic
4. **Immutable**: Once assigned, a version never changes
5. **No Gaps**: Version sequence is contiguous (no skipped numbers)

### Version Lifecycle

```
v1 → Publish → v2 → Publish → v3 → Publish → v4
                ↑                ↑                ↑
            (success)        (success)        (success)

Failed publish: Version NOT incremented
```

### Version 0

Reserved. First successful publish assigns version 1.

---

## Long-Polling Distribution

The relay uses `tokio::sync::Notify` to implement efficient long-polling without thread-per-connection overhead.

### Client Behavior

```
1. Runtime sends GET /v1/config?wait_ms=30000 with X-Pavis-Version: 42
2. If server_version > client_version → immediate 200 OK with new artifact
3. If server_version == client_version → register waiter, block up to 30s
4. On new publish → notify all waiters → 200 OK with new artifact
5. On timeout → 304 Not Modified
```

### Server State Machine

```
┌─ GET /v1/config ─────────────────────────────────┐
│                                                   │
├─ Parse X-Pavis-Version header                    │
│                                                   │
├─ Compare with current_version                    │
│   │                                               │
│   ├─ client_ver < current_ver                    │
│   │  └→ Immediate 200 OK + current artifact     │
│   │                                               │
│   ├─ client_ver == current_ver (+ wait_ms > 0)  │
│   │  └→ Register waiter                          │
│   │     ├─ Await Notify OR Timeout               │
│   │     ├─ On Notify → 200 OK + new artifact    │
│   │     └─ On Timeout → 304 Not Modified        │
│   │                                               │
│   └─ client_ver > current_ver                    │
│      └→ Immediate 304 Not Modified               │
└───────────────────────────────────────────────────┘
```

### Notification Mechanism

**On Publish:**
```rust
1. Serialize publish operation (mutex)
2. Validate PVS artifact
3. Increment version atomically
4. Update in-memory state (Arc<RwLock>)
5. Persist LKG to disk
6. notify_waiters() → wakes ALL long-poll waiters
7. Return 200 OK to publisher
```

**Performance:**
- **O(1) memory** per waiter
- **No thread-per-connection**
- **Sub-millisecond** notification latency

---

## Content-Addressed Identity

Artifact identity is defined by **checksum**, not version.

### Properties

1. **Immutable**: Artifact bytes never change after creation
2. **Content-Addressed**: Identity = SHA256(payload)
3. **Version-Independent**: Same bytes = same checksum across versions

### Implications

**Identical Artifacts:**
```
Publish config-v1.pvs → version 1, checksum abc123
Publish config-v1.pvs → version 2, checksum abc123  (same bytes)
Publish config-v2.pvs → version 3, checksum def456  (different bytes)
```

- Different versions MAY have same checksum
- Checksum is authoritative identity
- Version is ordinal for distribution only

**Deduplication:**
```
Runtime checks: current_checksum == new_checksum
If true: Skip reload (no-op)
If false: Apply new configuration
```

---

## Last Known Good (LKG)

The relay maintains a **Last Known Good** artifact representing the current configuration.

### LKG Properties

1. **Singleton**: Only one LKG exists at any time
2. **Atomic Updates**: LKG transitions are atomic (temp file + rename)
3. **Persistent**: Survives relay restarts
4. **Metadata**: Includes version, checksum, timestamp

### LKG Lifecycle

```
Publish
  ↓
Validate PVS
  ↓
Write temp file: /var/lib/pavis-relay/lkg/.tmp_config.pvs
  ↓
Write metadata: /var/lib/pavis-relay/lkg/.tmp_meta.json
  ↓
Atomic rename: .tmp_config.pvs → config.pvs
  ↓
Atomic rename: .tmp_meta.json → meta.json
  ↓
Update in-memory state
  ↓
Notify waiters
```

**Atomicity Guarantee:** Relay crash during publish leaves either old or new LKG, never partial state.

---

## Concurrency Model

### Publish Serialization

Publishes are serialized via internal mutex:

```rust
Publish Request 1 → [MUTEX] → validate → increment → persist → notify → response
Publish Request 2 ────────────────┘ (waits)
Publish Request 3 ───────────────────────┘ (waits)
```

**Rationale:**
- Version monotonicity requires serialization
- Disk writes benefit from serialization
- Publish rate is low (minutes to hours between publishes)

### Fetch Concurrency

Config fetches (GET /v1/config) are fully concurrent:

```rust
Read current state: Arc<RwLock<State>>.read()
  ↓
Zero-copy serve: Arc<Bytes> (shared ownership)
```

**Throughput:** 10k-50k RPS per relay instance

---

## Error Handling

### Publish Failures

| Error | Version Behavior | LKG Behavior | Waiter Behavior |
|-------|------------------|--------------|-----------------|
| Invalid PVS | NOT incremented | Unchanged | Not notified |
| Disk full | NOT incremented | Unchanged | Not notified |
| Checksum fail | NOT incremented | Unchanged | Not notified |

**Invariant:** Version only increments on successful publish.

### Fetch Failures

| Scenario | Response | Retry Strategy |
|----------|----------|----------------|
| Long-poll timeout | 304 Not Modified | Immediate retry with same version |
| Network error | - | Exponential backoff |
| Server unavailable | - | Exponential backoff |

---

## Performance Characteristics

### Latency

- **Publish Latency**: 10-50ms (includes PVS verification + disk sync)
- **Config Propagation**: <100ms (from publish to client notification)
- **Long-Poll Overhead**: <1ms per waiter (Notify is highly efficient)

### Throughput

- **Publish**: 100-500 RPS (limited by disk I/O)
- **Config Fetch**: 10k-50k RPS (memory-only, zero-copy)

### Scalability

- **Connected Clients**: 10k+ concurrent long-poll connections supported
- **Memory**: ~1KB per active long-poll waiter
- **Artifact Size**: Default limit 10 MB (configurable via `max_pvs_bytes`)

---

## Limitations

### Explicitly Not Supported

- **Rollback API**: No built-in rollback (clients fetch historical versions explicitly)
- **Multi-Tenancy**: Single configuration stream per relay instance
- **Partial Configs**: No incremental or differential updates
- **Push Notifications**: Long-polling only (no WebSocket/SSE)

---

## Related Documents

- **API Reference**: See `../api/relay.md` for HTTP endpoint details
- **Operations Guide**: See `../operations/relay.md` for deployment and monitoring
- **Architecture**: See `/ARCHITECTURE.md` for system invariants

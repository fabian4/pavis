# Relay HTTP API

> **Class:** API  
> **Question:** How do clients interact with the Relay HTTP API?  
> **Authority:** This document is normative. Implementation resides in code (`crates/pavis-relay`).

---

## Overview

The Pavis Relay exposes an HTTP API for configuration publishing and distribution. It serves as the central control plane for configuration management across a fleet of Pavis runtime instances.

**Base URL:** `http://127.0.0.1:8080` (configurable)

---

## Endpoints

### POST /v1/publish

Publishes a new configuration artifact to the relay.

**Request:**
```http
POST /v1/publish HTTP/1.1
Content-Type: application/octet-stream
Content-Length: <size>

<PVS artifact bytes>
```

**Response (200 OK):**
```json
{
  "version": 42,
  "checksum": "sha256:abc123...",
  "size": 1234,
  "published_at": "2026-01-18T08:00:00Z"
}
```

**Response Fields:**
- `version` (u64): Relay-generated version number (strictly monotonic)
- `checksum` (string): SHA256 checksum in format `sha256:{hex}`
- `size` (u64): Artifact size in bytes
- `published_at` (string): ISO 8601 timestamp

**Error Responses:**

| Status | Condition | Response Body |
|--------|-----------|---------------|
| 400 Bad Request | Invalid PVS artifact | `"verification failed: invalid magic bytes"` |
| 413 Payload Too Large | Exceeds `max_pvs_bytes` | `"pvs size X exceeds max_pvs_bytes Y"` |
| 500 Internal Server Error | Storage failure | `"failed to write LKG: ..."` |

**Semantics:**
- Version is **relay-generated** (clients cannot specify version)
- Versions are **strictly monotonic**: `new_version = current_version + 1`
- Publishing identical artifacts creates **distinct versions** with the **same checksum**
- On failure, version is **NOT incremented**
- Publishes are **serialized** (concurrent requests queued)

**Example:**
```bash
curl -X POST http://127.0.0.1:8080/v1/publish \
  --data-binary @config.pvs \
  -H "Content-Type: application/octet-stream"
```

---

### GET /v1/config

Fetches the current Last Known Good (LKG) configuration with optional long-polling.

**Request:**
```http
GET /v1/config?wait_ms=30000 HTTP/1.1
If-None-Match: "sha256:abc123..."
```

**Query Parameters:**
- `wait_ms` (optional, u64): Long-poll timeout in milliseconds
  - Range: `0..=60000` (values > 60000 return 400)
  - Default: `0` (no long-poll)

**Request Headers:**
- `If-None-Match` (optional): Strong ETag (`"sha256:<lowercase-hex>"`)

**Response (200 OK):**
```http
HTTP/1.1 200 OK
ETag: "sha256:abc123..."
x-config-version: 42
x-config-size: 1234
Content-Type: application/octet-stream
Content-Length: 1234

<PVS artifact bytes>
```

**Response (304 Not Modified):**
```http
HTTP/1.1 304 Not Modified
ETag: "sha256:abc123..."
```

Returned when:
- `wait_ms=0` and the conditional ETag matches the current artifact

**Semantics:**
- If `If-None-Match` is missing or invalid → unconditional GET (200 OK)
- If `If-None-Match` does not match current ETag → immediate 200 OK with artifact
- If `If-None-Match` matches current ETag and `wait_ms > 0` → long-poll:
  - Register waiter
  - Block up to `wait_ms` milliseconds
  - On publish with different checksum → 200 OK with new artifact
  - On timeout → 204 No Content
- If `If-None-Match` matches current ETag and `wait_ms = 0` → 304 Not Modified

**Example:**
```bash
# Immediate fetch
curl http://127.0.0.1:8080/v1/config

# Long-poll (blocks up to 30s if no update)
curl http://127.0.0.1:8080/v1/config?wait_ms=30000 \
  -H 'If-None-Match: "sha256:abc123..."'
```

---

### GET /v1/artifacts/:version

Retrieves a specific historical artifact by version number.

**Request:**
```http
GET /v1/artifacts/42 HTTP/1.1
```

**Response (200 OK):**
```http
HTTP/1.1 200 OK
X-Config-Version: 42
X-Pavis-Checksum: sha256:abc123...
X-Pavis-Checksum-Alg: sha256
X-Pavis-Generated-At: 2026-01-18T08:00:00Z
Content-Type: application/octet-stream
Content-Length: 1234

<PVS artifact bytes>
```

**Response (404 Not Found):**
```json
{"error": "version not found"}
```

**Semantics:**
- Historical artifacts are optional (configured via `relay.yaml`)
- If history is disabled, only current LKG is available
- Clients should NOT rely on historical versions being available

---

### GET /v1/status

Returns relay status and metadata.

**Response (200 OK):**
```json
{
  "status": "healthy",
  "uptime_s": 3600,
  "current_version": 42,
  "lkg": {
    "version": 42,
    "size": 1234,
    "checksum": "sha256:abc123...",
    "published_at": "2026-01-18T08:00:00Z"
  },
  "history_count": 10
}
```

---

### GET /health

Liveness probe for health checks.

**Response (200 OK):**
```
ok
```

**Semantics:**
- Always returns 200 OK if relay is running
- Suitable for Kubernetes liveness probes

---

### GET /metrics

Prometheus-formatted metrics endpoint.

**Response (200 OK):**
```
# TYPE pavis_relay_version gauge
pavis_relay_version 42

# TYPE pavis_relay_publish_ok_total counter
pavis_relay_publish_ok_total 100

# TYPE pavis_relay_publish_fail_total counter
pavis_relay_publish_fail_total 2

# TYPE pavis_relay_longpoll_wait_total counter
pavis_relay_longpoll_wait_total 5
```

**Key Metrics:**
- `pavis_relay_version`: Current configuration version (gauge)
- `pavis_relay_publish_ok_total`: Total successful publishes (counter)
- `pavis_relay_publish_fail_total`: Total failed publishes (counter)
- `pavis_relay_longpoll_wait_total`: Long-poll waits (counter)

---

## Configuration

The relay's HTTP server is configured via `relay.yaml`:

```yaml
http:
  bind: "0.0.0.0:8080"
```

---

## Related Documents

- **Protocol Specification**: See `../specs/relay-protocol.md` for distribution protocol semantics
- **Operations Guide**: See `../operations/relay.md` for deployment and monitoring
- **Architecture**: See `/ARCHITECTURE.md` for system invariants

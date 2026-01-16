# Relay API Reference

This document describes the HTTP API endpoints provided by `pavis-relay`.

## Overview

The relay serves as a central distribution point for Pavis runtime configuration. It provides:
- **Configuration publishing** (POST /v1/publish)
- **Configuration fetching** with long-polling (GET /v1/config)
- **Health/status monitoring** (GET /v1/status, /health, /ready)
- **Historical artifact retrieval** (GET /v1/artifacts/:version)

## Base URL

Default: `http://127.0.0.1:8080`

Configurable via `relay.yaml`:
```yaml
http:
  bind: "0.0.0.0:8080"
```

---

## Endpoints

### POST /v1/publish

Publishes a new Pavis configuration artifact to the relay.

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
  "published_at": "2026-01-16T12:00:00Z"
}
```

**Response Fields:**
- `version` (u64): Relay-generated version number (monotonically increasing)
- `checksum` (string): SHA256 checksum of the artifact in format `sha256:{hex}`
- `size` (u64): Size of the artifact in bytes
- `published_at` (string): ISO 8601 timestamp of publication

**Error Responses:**

| Status | Condition | Body Example |
|--------|-----------|--------------|
| 400 Bad Request | Invalid PVS artifact | `"verification failed: invalid magic bytes"` |
| 413 Payload Too Large | Exceeds `max_pvs_bytes` | `"pvs size 2048 exceeds max_pvs_bytes 1024"` |
| 500 Internal Server Error | Storage failure | `"failed to write LKG: ..."` |

**Semantics:**
- Version is **relay-generated** (clients cannot specify version)
- Versions are **strictly monotonic**: `new_version = current_version + 1`
- Publishing identical artifacts creates **distinct versions** with the **same checksum**
- On failure, version is **NOT incremented**
- Publishes are **serialized** via internal mutex (concurrent requests are queued)

**Example (using curl):**
```bash
curl -X POST http://127.0.0.1:8080/v1/publish \
  --data-binary @config.pvs \
  -H "Content-Type: application/octet-stream"
```

**Example (using pavctl):**
```bash
pavctl publish --relay http://127.0.0.1:8080 config.pvs
```

---

### GET /v1/config

Fetches the current Last Known Good (LKG) configuration with long-polling support.

**Request:**
```http
GET /v1/config?timeout=30 HTTP/1.1
```

**Query Parameters:**
- `timeout` (optional, u64): Long-poll timeout in seconds
  - Default: `30`
  - Range: `[1, 60]` (values outside range return 400)

**Response (200 OK):**
```http
HTTP/1.1 200 OK
Content-Type: application/octet-stream
X-Config-Checksum: sha256:abc123...
X-Config-Size: 1234
X-Config-Version: 42
X-Pavis-Generated-At: 2026-01-16T12:00:00Z
Cache-Control: no-store

<PVS artifact bytes>
```

**Response Headers:**
- `X-Config-Checksum` (string): SHA256 checksum of the response body (use for change detection)
- `X-Config-Size` (u64): Size of the artifact in bytes
- `X-Config-Version` (u64): Relay version number (**observability only, do NOT use for change detection**)
- `X-Pavis-Generated-At` (string): ISO 8601 timestamp when config was last updated
- `Cache-Control`: Always `no-store` (config must be fetched fresh)

**Long-Poll Behavior:**
- If a publish occurs while request is waiting → wake immediately, return new LKG
- If timeout expires → return current LKG (may be identical to previous poll)
- Clients **MUST** use `X-Config-Checksum` for change detection (not version)

**Client Responsibilities:**
1. **Extract `X-Config-Checksum` from response headers**
2. **Compute `sha256(response_body)` and verify it matches header checksum**
   - If mismatch: **fail-close** (do not apply; log error; retry)
3. **Compare checksum with previous value**
   - If different → apply new config
   - If same → skip update (no change)
4. **Accept that timeouts may return unchanged config** (idempotent operation)

**Error Responses:**

| Status | Condition | Body Example |
|--------|-----------|--------------|
| 400 Bad Request | Invalid timeout | `"timeout must be within [1, 60]"` |

**Example (using curl):**
```bash
curl -i http://127.0.0.1:8080/v1/config?timeout=30 -o config.pvs
```

**Example (checksum verification in bash):**
```bash
# Fetch config and extract checksum
CHECKSUM=$(curl -si http://127.0.0.1:8080/v1/config | grep -i x-config-checksum | cut -d' ' -f2 | tr -d '\r')
curl -s http://127.0.0.1:8080/v1/config -o config.pvs

# Verify checksum
COMPUTED=$(sha256sum config.pvs | awk '{print "sha256:"$1}')
if [ "$CHECKSUM" = "$COMPUTED" ]; then
  echo "Checksum verified: $CHECKSUM"
else
  echo "ERROR: Checksum mismatch!"
  exit 1
fi
```

---

### GET /v1/status

Returns relay health and versioning metadata.

**Request:**
```http
GET /v1/status HTTP/1.1
```

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
    "published_at": "2026-01-16T12:00:00Z"
  },
  "history_count": 42
}
```

**Response Fields:**
- `status` (string): Always `"healthy"` (future: may return degraded states)
- `uptime_s` (u64): Relay uptime in seconds since process start
- `current_version` (u64): Current relay version (from LKG metadata)
- `lkg` (object, optional): Last Known Good metadata
  - `null` if no config has been published (bootstrap state)
  - Fields match `ArtifactMetadata` structure
- `history_count` (u64): Number of historical versions stored in `history/`

**Example:**
```bash
curl http://127.0.0.1:8080/v1/status | jq .
```

---

### GET /health

Liveness probe endpoint (always returns success).

**Request:**
```http
GET /health HTTP/1.1
```

**Response (200 OK):**
```
ok
```

**Use Case:**
- Kubernetes liveness probe
- Load balancer health check
- Always returns 200 OK (relay process is alive)

**Example:**
```bash
curl http://127.0.0.1:8080/health
```

---

### GET /ready

Readiness probe endpoint (checks if relay has valid configuration).

**Request:**
```http
GET /ready HTTP/1.1
```

**Response (200 OK):**
```
ready
```

**Response (503 Service Unavailable):**
```
no artifact
```

**Semantics:**
- Returns 200 if LKG artifact exists (relay is ready to serve config)
- Returns 503 if no artifact published yet (bootstrap state)

**Use Case:**
- Kubernetes readiness probe
- Circuit breaker logic
- Determines if relay can serve traffic

**Example:**
```bash
curl -f http://127.0.0.1:8080/ready || echo "Relay not ready"
```

---

### GET /v1/artifacts/:version

Fetches a specific historical artifact by version number.

**Request:**
```http
GET /v1/artifacts/42 HTTP/1.1
```

**Response (200 OK):**
```http
HTTP/1.1 200 OK
Content-Type: application/octet-stream
X-Pavis-Version: 42
X-Pavis-Checksum: abc123...
X-Pavis-Checksum-Alg: sha256
X-Pavis-Generated-At: 2026-01-16T12:00:00Z
Cache-Control: no-store

<PVS artifact bytes for version 42>
```

**Response Headers:**
- Standard Pavis metadata headers (version, checksum, etc.)

**Error Responses:**

| Status | Condition | Body Example |
|--------|-----------|--------------|
| 404 Not Found | Version does not exist | `"unknown version"` |

**Example:**
```bash
curl http://127.0.0.1:8080/v1/artifacts/1 -o version-1.pvs
```

---

### GET /v1/metrics

Prometheus-compatible metrics endpoint.

**Request:**
```http
GET /v1/metrics HTTP/1.1
```

**Response (200 OK):**
```prometheus
# HELP pavis_relay_version Current config version
# TYPE pavis_relay_version gauge
pavis_relay_version 42
# HELP pavis_relay_publish_ok_total Successful publishes
# TYPE pavis_relay_publish_ok_total counter
pavis_relay_publish_ok_total 42
# HELP pavis_relay_publish_fail_total Failed publishes
# TYPE pavis_relay_publish_fail_total counter
pavis_relay_publish_fail_total 3
# HELP pavis_relay_longpoll_wait_total Long poll waits
# TYPE pavis_relay_longpoll_wait_total counter
pavis_relay_longpoll_wait_total 1234
```

**Metrics:**
- `pavis_relay_version`: Current version (gauge)
- `pavis_relay_publish_ok_total`: Successful publish count (counter)
- `pavis_relay_publish_fail_total`: Failed publish count (counter)
- `pavis_relay_longpoll_wait_total`: Long-poll request count (counter)

**Example:**
```bash
curl http://127.0.0.1:8080/v1/metrics
```

---

## Versioning Model

### Version Generation

- Versions are **relay-generated** (clients cannot propose versions)
- Versions are **strictly monotonic**: `new_version = current_version + 1`
- Version `0` is a sentinel representing "no published configuration" (bootstrap state)
- Relay version ≠ PVS schema version (independent concerns)

### Version Persistence

- **Authoritative source**: `lkg/meta.json` (LKG metadata file)
- **Cache**: `state.json` (derived from LKG, rewritten on mismatch)
- On startup: version is loaded from `lkg/meta.json`, not `state.json`

### Idempotency

Publishing the **same artifact twice** creates:
- **Two distinct versions** (e.g., v1 and v2)
- **Same checksum** (identical bytes)

This is **correct behavior** (not a bug).

---

## Checksum Format

All checksums follow the format: `sha256:{64 hex chars}`

**Example:**
```
sha256:a3f8d7e2c1b9f6e4d8c7a5b3f1e9d7c5b4a2f8e6d4c2b1a9f7e5d3c1b9a7f5e3
```

**Computation:**
```rust
let digest = sha2::Sha256::digest(bytes);
let checksum = format!("sha256:{}", hex::encode(digest));
```

**Verification (clients MUST do this):**
```rust
let header_checksum = response.headers().get("X-Config-Checksum")?;
let body_bytes = response.bytes().await?;
let computed = sha256(body_bytes);
assert_eq!(header_checksum, computed, "Checksum mismatch - fail closed!");
```

---

## Long-Polling Pattern

### Client Implementation

```rust
loop {
    let response = client.get("http://relay:8080/v1/config?timeout=30").await?;
    let header_checksum = response.headers().get("X-Config-Checksum")?.to_str()?;

    // Check if changed
    if Some(header_checksum) == last_checksum {
        continue; // No change, poll again
    }

    // Verify checksum
    let body = response.bytes().await?;
    let computed = sha256(&body);
    if computed != header_checksum {
        eprintln!("Checksum mismatch - corruption detected!");
        continue; // Fail-close, retry
    }

    // Apply new config
    apply_config(&body)?;
    last_checksum = Some(header_checksum.to_string());
}
```

### Server Behavior

1. Client sends `GET /v1/config?timeout=30`
2. Relay checks if publish event occurs within 30 seconds
3. If publish → wake immediately, return new LKG
4. If timeout → return current LKG (may be unchanged)
5. Client compares checksum to detect changes

---

## Error Handling

### Client-Side

- **400 Bad Request**: Fix request parameters (timeout, etc.)
- **413 Payload Too Large**: Reduce artifact size or increase `max_pvs_bytes`
- **500 Internal Server Error**: Retry with exponential backoff
- **Checksum Mismatch**: **Fail-closed** (do not apply corrupted config)

### Server-Side

- **Publish Failures**: Version NOT incremented, history entry rolled back
- **Storage Errors**: Logged, publish rejected
- **Validation Errors**: 400 returned, no state changed

---

## Rate Limiting

Currently **not implemented**. Recommendations:

- Use reverse proxy (nginx, envoy) for rate limiting
- Monitor `pavis_relay_publish_fail_total` for abuse
- Implement application-level limits in future versions

---

## Security Considerations

### Checksum Verification

Clients **MUST** verify checksums to detect:
- Man-in-the-middle attacks
- Corrupted network transfers
- Storage corruption

### No Authentication

Current implementation has **no authentication**. Recommendations:

- Deploy relay in trusted network (internal cluster)
- Use mTLS for client authentication (future)
- Use network policies to restrict access

### Fail-Closed Policy

On checksum mismatch, clients **MUST**:
- **NOT apply** the config
- Log error for investigation
- Continue serving previous valid config
- Retry fetching from relay

---

## Best Practices

1. **Always verify checksums** before applying config
2. **Use long-polling** instead of frequent polling (reduces load)
3. **Monitor metrics** (`/v1/metrics`) for publish failures
4. **Set appropriate timeouts** (30-60s recommended for long-poll)
5. **Implement exponential backoff** on errors
6. **Test fail-closed behavior** (simulate checksum mismatches)

---

## Examples

### Publishing with pavctl

```bash
# Generate config
pavctl gen --output config.pvs config.yaml

# Publish to relay
pavctl publish --relay http://relay:8080 config.pvs
```

**Output:**
```
Published config to relay
  Version:      1
  Checksum:     sha256:a3f8d7e2c1b9f6e4d8c7a5b3f1e9d7c5b4a2f8e6d4c2b1a9f7e5d3c1b9a7f5e3
  Size:         2048 bytes
  Published At: 2026-01-16 12:00:00 UTC
```

### Fetching with ConfigAgent (Runtime)

```rust
let agent = ConfigAgent::new(
    "http://relay:8080".to_string(),
    lkg_path,
    state,
    timeout,
    backoff,
).await?;

loop {
    match agent.poll_once().await {
        Ok(PollOutcome::Updated) => {
            tracing::info!("Config updated successfully");
        }
        Ok(PollOutcome::NoChange) => {
            tracing::debug!("No config change");
        }
        Err(err) => {
            tracing::error!("Poll failed: {}", err);
            tokio::time::sleep(backoff.next()).await;
        }
    }
}
```

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-16 | Initial API specification with relay-generated versioning and checksum-based change detection |

---

## See Also

- [Operational Guide](../operations/relay.md)
- [Crash Recovery](../operations/crash-recovery.md)
- [Architecture Documentation](../../ARCHITECTURE.md)

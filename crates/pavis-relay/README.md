## Pavis Relay HTTP API Contract

### Scope
Relay serves versioned .pvs artifacts over HTTP long-poll. Runtime/sidecars are read-only. Publish is control-plane only. Relay coordinates ingest + codec and MUST NOT parse DTOs.

### Rules
- Accept only .pvs bytes; no DTOs.
- Validate PVS magic/version/checksum on publish.
- Include version + checksum headers on artifact responses.

### Headers
- X-Pavis-Version: current relay version.
- X-Pavis-Checksum: payload checksum (header excluded).
- X-Pavis-Checksum-Alg: checksum algorithm.

### GET /v1/config
- Purpose: fetch current .pvs, long-poll if up-to-date.
- Request: X-Pavis-Version; wait_ms query; empty body.
- Response: 200 .pvs + headers; 304/204 timeout; 400 bad header; 500 error.
- Notes: hold when versions match; return immediately on update; add Cache-Control: no-store.

### GET /v1/artifacts/:version
- Purpose: fetch historical artifact by version.
- Request: path version.
- Response: 200 .pvs + headers; 404 unknown; 400 invalid; 500 error.

### POST /v1/publish
- Purpose: publish new .pvs (control plane).
- Request: X-Pavis-Version; body is raw .pvs bytes.
- Response: 200 ok; 400 missing/empty; 409 non-monotonic; 422 integrity failure; 500 error.
- Side effects: update active version, notify waiters, persist if enabled.

### GET /v1/status
- Purpose: operational status.
- Response: 200 version/checksum/size/last update; 500 error.

### GET /v1/metrics
- Purpose: Prometheus metrics.
- Response: 200 text format; 500 error.

### GET /health
- Purpose: liveness probe.
- Response: 200 ok.

### GET /ready
- Purpose: readiness probe.
- Response: 200 ready; 503 when no artifact/LKG or storage failure.

### Caching/ETag
- Use Cache-Control: no-store on artifact responses.
- If ETag is used, derive it from the payload checksum.

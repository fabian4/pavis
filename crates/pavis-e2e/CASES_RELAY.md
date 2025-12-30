# Relay E2E Cases

## Relay Plan (Relay-Only)

### R1: Publish increments artifact version and updates LKG atomically
- Setup: start relay with empty storage in temp dir.
- Action: `POST /v1/publish` with valid `.pvs` bytes (config A).
- Expect:
  - Response: 200, headers `X-Pavis-Version: 1`,
    `X-Pavis-Checksum: <hex>`.
  - `GET /v1/artifacts/1` returns `.pvs` with correct PVS schema version header.
  - `GET /v1/status` includes `version=1`.
- Publish config B.
- Expect: artifact version increments to 2; LKG becomes 2 only after full write.
- Invariant: no partial artifact visible; LKG changes atomically.

### R2: Reject invalid `.pvs`
- Action: `POST /v1/publish` with corrupted `.pvs` bytes.
- Expect: 422; `GET /v1/status` shows LKG unchanged.

### R3: Long-poll semantics
- Action: `GET /v1/config?wait_ms=1000` with `X-Pavis-Version: 1` and no new version.
- Expect: holds until timeout, then `304 Not Modified`.
- Publish version 2 while client is waiting.
- Expect: long-poll returns immediately with new artifact headers and full `.pvs`
  body.

### R4: Partial write protection
- Failure injection strategy (filesystem-based):
  - Make the artifacts directory read-only just before publish, or create a
    read-only file at the intended final artifact path to force rename failure.
- Expect: artifact remains at previous version; LKG not updated.

### R5: Observability
- Check `GET /v1/metrics` counters:
  - `pavis_relay_publish_total`
  - `pavis_relay_publish_fail_total`
  - `pavis_relay_longpoll_wait_total`
- Check `GET /v1/status` includes `version=` and `checksum=`.

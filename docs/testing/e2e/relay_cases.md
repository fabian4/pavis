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

### R6: Ingest Debouncing
- Setup: Relay with `file` ingest and debounce=200ms.
- Action: Write to watched file 5 times within 100ms.
- Expect:
  - Only one version increment (e.g., v1 -> v2, not v1 -> v6).
  - Logs show "Debounce expired, reading file".

### R7: Persistence Recovery (Crash Safety)
- Setup: Start Relay, publish v1.
- Action: Kill Relay process (`SIGKILL`). Restart Relay with same storage/LKG path.
- Expect:
  - `GET /v1/status` immediately returns version 1.
  - LKG file on disk is valid and matches v1 checksum.

### R8: Codec Validation (Pipeline)
- Setup: Relay with `file` ingest.
- Action: Write invalid YAML (syntax error or schema violation) to watched file.
- Expect:
  - Relay logs error (parsing or validation).
  - `GET /v1/status` version does *not* increment.
  - Runtime (if connected) does not receive an update.

### R9: File Replacement (Editor Simulation)
- Setup: Relay watching `input.yaml`.
- Action: Move new file over old file (`mv new.yaml input.yaml`) or delete and recreate.
- Expect:
  - Watcher detects `Rename` or `Create` event.
  - Pipeline triggers and updates config.

### R10: Startup with Corrupted LKG
- Action: Create `lkg.pvs` with random garbage bytes.
- Action: Start Relay.
- Expect:
  - Relay fails to start (process exit code != 0).
  - Logs indicate "failed to initialize relay state" or PVS validation error.

### R11: Rapid Toggle (Flapping)
- Setup: Relay watching `input.yaml`.
- Action: Script that writes Valid -> Invalid -> Valid YAML in quick succession (500ms intervals).
- Expect:
  - Relay processes first Valid.
  - Relay rejects Invalid (logs error).
  - Relay processes second Valid.
  - Final version increment matches number of valid updates (or less if debounced).

### R12: Symlink Updates (Kubernetes ConfigMap)
- Setup: `input.yaml` is a symlink to `data/v1.yaml`.
- Action: Update symlink `input.yaml` -> `data/v2.yaml`.
- Expect:
  - Watcher detects modification.
  - Config updates to v2 content.

### R13: Transient File Permission Failure
- Setup: Relay watching `input.yaml`.
- Action: `chmod 000 input.yaml`. Wait > debounce. `chmod 644 input.yaml`.
- Expect:
  - Relay logs error on first attempt (Permission Denied).
  - Relay *recovers* or processes subsequent events successfully once permissions are restored.

### R14: Transient Empty File
- Setup: Relay watching `input.yaml`.
- Action: `truncate -s 0 input.yaml`. Wait > debounce. Write valid config.
- Expect:
  - Relay logs error on empty file (Codec failure) or ignores it.
  - Relay successfully processes the subsequent valid write.
  - Process does *not* crash.

### R15: Artifact Size Limits
- Setup: Relay config `artifact.limits.max_pvs_bytes = 100`.
- Action: Ingest valid config that compiles to > 100 bytes.
- Expect:
    - Relay logs "Artifact size exceeded" error.
    - Version does not increment.
  
  ### R16: Traceability
  - Action: Fetch configuration or artifact.
  - Expect:
    - Header `X-Pavis-Generated-At` is present.
    - Value is a valid RFC3339 timestamp.
  
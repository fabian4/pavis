# Task: Implement Relay Pipeline Orchestration

## 1. Requirements

**Purpose**: Integrate the Ingest and Codec layers into `pavis-relay` to enable automatic PVS generation and distribution.

**Functionality**:
- **Pipeline Task**: A background task that runs for the lifetime of the relay.
- **Auto-Discovery**: Support `file-serde` pipeline (initially) where `pavis-ingest-file` feeds `pavis-codec-serde`.
- **Auto-Publish**: Automatically publish a new `.pvs` to the `RelayState` whenever the pipeline yields a new valid configuration.
- **Binary Mode**: Must be fully testable and runnable in "Binary Mode" (standalone binaries, no Docker).

**Constraints**:
- **Strict Dependencies**: The Relay must use the `Codec` and `Ingest` traits from the `-api` crates.
- **Error Resilience**: A compilation or validation failure in the pipeline must NOT crash the relay. It should log the error and continue watching for the next update.

---

## 2. Guidelines

- **Architecture**:
  - Implement pipeline logic in `crates/pavis-relay/src/pipeline/`.
  - Use `tokio::spawn` to run the pipeline background loop.
- **Feature Flags**: Use Cargo features (`plugin-file-yaml`, etc.) as defined in `ARCHITECTURE.md` to enable specific pipelines.
- **Validation**: Every configuration produced by the codec MUST be validated via `pavis_core::validate_runtime` before being encoded to `.pvs`.

---

## 3. Design Document

### Architecture Design
```text
[Source File] --(events)--> [Ingest: File] --(Artifact)--> [Codec: Serde]
                                                               │
                                                               ▼
[RelayState] <--(Publish)-- [.pvs Bytes] <--(Encode)-- [ValidatedRuntimeConfig]
```

### Integration Points
1. **RelayState**: Add a method to `RelayState` for internal publishing (bypass HTTP).
2. **App Startup**: In `serve_from_config`, check if a pipeline is configured and start it.

### Error Handling
- **Ingest Error**: Log and retry/continue.
- **Codec Error**: Log and ignore the specific artifact.
- **Validation Error**: Log "Invalid config from source" and ignore.

---

## 4. Acceptance Criteria

- **Functionality**:
  - Relay starts and immediately picks up config from the configured file.
  - Updating the file results in a new version in `X-Pavis-Version` header for long-pollers.
- **E2E Integration**:
  - Passes `pavis-e2e` relay tests in binary mode.
- **Performance**: PVS generation happens off the request-response hot path.
- **Stability**: Hand-editing a file to be syntactically invalid does not crash the relay; fixing the file restores functionality.

---

## 5. E2E Tests (File Watch Scenario)

- **Scenario**: `relay_file_watch_updates`
  1. Start `pavis-relay` with `ingest.kind: file` pointing to `test.yaml`.
  2. Write a valid v1 config to `test.yaml`.
  3. Verify `GET /v1/status` shows version 1.
  4. Write a valid v2 config to `test.yaml`.
  5. Verify `GET /v1/status` shows version 2 within 1 second.
- **Scenario**: `relay_file_watch_recovery`
  1. Start relay with valid file.
  2. Overwrite file with junk text.
  3. Verify relay logs error but stays alive (version stays at v1).
  4. Overwrite with valid v3.
  5. Verify relay recovers and serves v3.

---

## 6. Test Cases

| Category | Case | Expected Result |
| :--- | :--- | :--- |
| **Functional** | Pipeline Auto-start | Relay loads config on start without manual `POST /publish`. |
| **Functional** | Multi-hop Update | Sequence of updates is correctly versioned (v1 -> v2 -> v3). |
| **Boundary** | File Deleted | Relay keeps serving Last Known Good (LKG). |
| **Negative** | Codec Mismatch | Configuration fails fast if `XdsState` is fed to `SerdeCodec`. |
| **Regression** | Binary Mode E2E | Run with `TEST_MODE=binary make e2e-relay`. |

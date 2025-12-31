# Task: Implement `pavis-ingest-file`

## 1. Requirements

**Purpose**: Develop a file-system ingestion layer that watches local files and emits their content as `pavis_ingest_api::Artifact` objects.

**Integration**: This crate must integrate with `pavis-relay` via the pipeline orchestration (see `FEAT_RELAY_PIPELINE.md`).

**Inputs**:
- Configuration specifying the file path to watch.
- Debounce interval (optional, default 100ms).

**Outputs**:
- A stream of `pavis_ingest_api::Artifact` items triggered by file changes.

**Constraints**:
- **Reliability**: Must handle transient file-system states (e.g., partial writes during save).
- **Strict Format Support**: ONLY `.yaml`, `.yml`, and `.json` are supported. All other file extensions must be rejected.
- **Async**: Must be fully asynchronous using `tokio` and `notify`.

---

## 2. Guidelines

- **Standards**: Adhere to `Architecture.md` (Modular Ingest Pipeline).
- **Architecture**:
  - Implement the `Ingest` trait from `pavis-ingest-api`.
  - Use the `notify` crate for cross-platform file system watching.
- **Dependencies**:
  - `notify` (recommended: version 6+ with `tokio` integration).
  - `futures-util` for stream handling.
  - `tokio` for I/O and timers.
- **Safety**: Ensure file handles are closed promptly; handle `PermissionDenied` and `NotFound` gracefully by retrying or logging.

---

## 3. Design Document

### Architecture Design
The ingestor works as a background stream generator:
1. **Watcher Setup**: Initialize a `notify` watcher on the target path.
2. **Event Filtering**: Listen for `Write`, `Create`, and `Rename` events.
3. **Format Validation**: On every event, check the file extension. If unsupported, log a warning and do NOT emit an artifact.
4. **Debounce Logic**: Use a timer to wait for a short period after the last event to ensure the file write is complete.
5. **Emit**: Read the file into `Bytes`, determine `Format`, and yield an `Artifact`.

### Data Models
The ingestor populates `Artifact` metadata:
- `format`: Inferred from `.yaml`, `.yml`, or `.json`.
- `source`: `SourceInfo` with the absolute path and file-system labels.
- `received_at`: Timestamp of the disk read.

---

## 4. Acceptance Criteria

- **Functionality**:
  - Emits an `Artifact` immediately upon starting if the file is valid.
  - Emits a new `Artifact` whenever the file content changes.
- **Debouncing**: Rapid successive saves (e.g., from an IDE) must result in a single `Artifact` emission once the file stabilizes.
- **Strict Format Detection**:
  - `.yaml` / `.yml` -> `Format::Yaml`.
  - `.json` -> `Format::Json`.
  - **Rejection**: Any other extension (e.g., `.toml`, `.txt`, `.conf`, `.xml`) must NOT result in an `Artifact` emission.
- **Robustness**: Does not crash if the file is deleted; resumes watching if the file is recreated.

---
...
## 6. Test Cases

| Category | Case | Expected Result |
| :--- | :--- | :--- |
| **Functional** | File content change | Stream yields updated `Artifact`. |
| **Boundary** | Zero-byte file | Stream yields empty `Artifact`. |
| **Boundary** | Unsupported Extension (`.txt`) | No `Artifact` emitted; log warning. |
| **Negative** | Permission Denied | `IngestError::Io` emitted to stream. |
| **Regression** | Rapid Save (Debounce) | Exactly one `Artifact` emitted for a burst of writes. |
| **Regression** | Symbolic Link | Correctly follows and watches the target of a symlink. |
| **Regression** | Binary Mode E2E | Run with `TEST_MODE=binary make e2e-relay`. |


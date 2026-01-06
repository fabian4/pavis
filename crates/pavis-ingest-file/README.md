# Pavis File Ingest

## 1. Crate Overview
`pavis-ingest-file` is an implementation of the `Ingest` trait that monitors the local filesystem for configuration changes. It provides a real-time, event-driven stream of `Artifact` objects, allowing Pavis to hot-reload configuration whenever a watched file is modified.

Its primary responsibilities are:
- Watching a specific file path for modifications using OS-native events.
- Debouncing rapid file changes to prevent partial reads or excessive reloading.
- Inferring configuration formats (YAML/JSON) from file extensions.
- Validating file integrity (UTF-8 check, non-empty check) before emitting artifacts.
- Providing a polling fallback for filesystems that do not support native event notifications.

It explicitly does not handle:
- Parsing the configuration content (delegated to `pavis-codec-*`).
- Managing multiple files (it watches a single authoritative path).

## 2. Features
- **Event-Driven**: Uses the `notify` crate to leverage cross-platform filesystem notifications (inotify, FSEvents, ReadDirectoryChangesW), ensuring near-instant updates.
- **Configurable Debounce**: Implements a timer-based debounce mechanism to wait for file I/O to stabilize (e.g., during an atomic write/rename) before reading.
- **Robustness**: Combines event-based watching with a 2-second polling interval to handle edge cases where events might be missed or are not supported.
- **Auto-Detection**: Maps `.yaml`, `.yml`, and `.json` extensions to their respective `pavis-ingest-api::Format` variants.

## 3. Module Breakdown

### `lib.rs`
The entry point for the file ingestor.
- `FileIngest`: The struct implementing the `Ingest` trait.
- `FileIngestStream`: The `futures_util::Stream` implementation that yields file updates.
- `infer_format`: Logic for mapping file extensions to formats.
- `validate_bytes`: Basic sanity checks for UTF-8 and content presence.

### `watch.rs`
Contains the background monitoring logic.
- `spawn_watcher`: Spawns a Tokio task that orchestrates the `notify` watcher and the debounce timer. It handles the `select!` loop between OS events, polling intervals, and the debounce timer.

## 4. Public API Surface

### `FileIngest`
The primary constructor for file-based ingestion.
- `new(path: impl Into<PathBuf>, debounce_duration: Duration)`: Configures which file to watch and how long to wait after a change before emitting.
- `stream()`: Starts the background watcher and returns the update stream.

### `spawn_watcher`
A lower-level helper for spawning the background task manually if needed, returning the `RecommendedWatcher` handle.

## 5. Configuration and Runtime Behavior

### Startup Behavior
- **Initial Load**: Upon calling `stream()`, the ingestor immediately attempts to read the file and emit the first `Artifact` (or an error) before starting the watcher.
- **Path Existence**: The target file must exist at the time `stream()` is called, as `notify` requires a valid path to start watching.

### Monitoring Strategy
1. **Primary**: OS events (`Modify`, `Create`, `Any`).
2. **Fallback**: Periodic 2-second poll that checks file modification times (`mtime`).
3. **Debounce**: Any detection (event or poll) resets the debounce timer. Only when the timer expires is the file read and sent to the stream.

## 6. Error Handling and Invariants

### Error States
- **Unsupported Format**: Emitted if the file extension is not `.yaml`, `.yml`, or `.json`.
- **Malformed Content**: Emitted if the file is not valid UTF-8 or is empty (whitespace-only).
- **IO Failure**: Emitted if the file becomes unreadable (e.g., permission changes).

### Invariants
- **Graceful Shutdown**: The background task automatically terminates when the `FileIngestStream` is dropped, closing the watcher.
- **Non-Blocking**: All I/O operations (reads, metadata checks) are performed using `tokio::fs` to avoid blocking the executor.

## 7. Non-Goals and Explicit Limitations
- **Directory Watching**: This crate watches a single file, not entire directories.
- **Secret Handling**: Does not provide special handling for secrets (e.g., masking or secure memory); it treats all file content as raw bytes.
- **Complex Formats**: Only supports UTF-8 text-based formats (YAML/JSON).

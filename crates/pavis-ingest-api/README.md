# Pavis Ingest API

## 1. Crate Overview
`pavis-ingest-api` defines the interface for retrieving raw configuration data from external sources. It provides the abstractions necessary to decouple *how* configuration is fetched (file watch, HTTP polling, gRPC stream) from *what* that configuration contains.

Its primary responsibilities are:
- Defining the `Ingest` trait for creating asynchronous configuration streams.
- Providing the `Artifact` struct, which encapsulates raw configuration bytes along with metadata (source, version, format).
- Categorizing ingestion failures via `IngestError`.
- Standardizing metadata via `SourceInfo` and `Format`.

It explicitly does not handle:
- Parsing or validating the configuration content (delegated to `pavis-codec-*`).
- Implementing specific transport logic (delegated to `pavis-ingest-file`, etc.).

## 2. Features
- **Transport Agnostic**: The `Ingest` trait relies on `futures_core::Stream`, allowing implementations to push updates via any mechanism (filesystem events, network push, timer poll).
- **Rich Metadata**: The `Artifact` type captures provenance (`SourceInfo`), versioning (`etag`, `version`), and content types, enabling downstream components to make informed decisions about processing.
- **Unified Error Handling**: `IngestError` provides specific variants for common failure modes (Auth, Transport, Backoff) to support sophisticated retry policies in the runtime.

## 3. Module Breakdown

### `lib.rs`
Contains the complete API definition.
- `Ingest`: The primary trait for data sources.
- `Artifact`: The unit of data transfer.
- `Format`: Enumeration of known configuration formats (Yaml, Json, Xds, Crd).
- `SourceInfo`: Metadata about the origin of the configuration (e.g., filename, URL).
- `IngestError`: A structured error type for the ingestion layer.

## 4. Public API Surface

### `Ingest` Trait
`async fn stream(&mut self) -> Result<Self::Stream, IngestError>`
Establishes a connection to the configuration source and returns a stream of `Result<Artifact, IngestError>`.

### `Artifact`
A container for configuration data.
- `bytes`: The raw content (`bytes::Bytes`).
- `format`: The declared format of the bytes.
- `source`: Where the bytes came from.
- `version` / `etag`: Optional concurrency control tokens.

### `Format`
Supported ingestion formats:
- `Yaml`, `Json`: Standard text formats.
- `XdsDelta`, `XdsState`: xDS protocol payloads.
- `Crd`: Kubernetes Custom Resource Definitions.

## 5. Configuration and Runtime Behavior
This crate defines interfaces only. Runtime behavior depends on the specific implementation (e.g., polling interval for a file ingestor).

### Streaming Semantics
- **Push-Based**: The API is designed for push-based updates. Implementations should yield a new `Artifact` whenever the source changes.
- **Error Propagation**: Errors in the stream (like a network disconnect) should be yielded as `Err(IngestError)`, allowing the consumer to decide whether to retry or terminate.

## 6. Error Handling and Invariants

### Error Types
- `Io`: Local filesystem or socket errors.
- `Transport`: Protocol-level failures (HTTP 500, gRPC status).
- `Auth`: Credential failures.
- `Backoff`: Errors related to retry limits.

### Invariants
- **Immutable Artifacts**: `Artifact` fields are public but the struct is generally treated as immutable once created.
- **Time Stamping**: Every `Artifact` is automatically stamped with `received_at` upon creation.

## 7. Non-Goals and Explicit Limitations
- **decoding**: This crate does not interpret the `bytes`. It only delivers them.
- **State Management**: It does not track "current" state; it is a stream of events.

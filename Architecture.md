# Architecture

## 1. Architecture Overview

Pavis replaces the traditional "Smart Proxy" model (Envoy) with a **Split Data Plane** architecture. Heavy lifting like parsing, defaulting, and semantic validation is offloaded to the ingest + codec pipeline, keeping the sidecar proxy lightweight and binary-focused while the relay focuses on artifact distribution.

### 1.1. End-to-End Configuration Flow

```text
  Source Type         Connectivity (Ingest)      Transformation (Codec)      Distribution & Tools           Data Plane
┌──────────────┐      ┌──────────────────┐                                  ┌────────────────────┐      ┌──────────────────┐
│ Mesh (Istio) │─────▶│ pavis-ingest-xds │──┐   ┌────────────────────┐  ┌──▶│    pavis-relay     │─────▶│    pavis-pvs     │
└──────────────┘      └──────────────────┘  ├──▶│  pavis-codec-xds   │──│   │ (Artifact Engine)  │  ┌──▶│ (Binary Protocol)│
┌──────────────┐      ┌──────────────────┐  │   └────────────────────┘  │   └──────────▲─────────┘  │   └──────────────────┘
│ Mesh (Kuma)  │─────▶│ pavis-ingest-xds │──┘                           │              │            │             
└──────────────┘      └──────────────────┘                              │              │            │             
┌──────────────┐      ┌──────────────────┐      ┌────────────────────┐  │              │ (apply)    │   ┌──────────────────┐
│ Kubernetes   │─────▶│ pavis-ingest-k8s │─────▶│  pavis-codec-crd   │──│              │            └──▶│      pavis       │
└──────────────┘      └──────────────────┘      └────────────────────┘  │   ┌──────────┴─────────┐      │    (Runtime)     │
┌──────────────┐      ┌───────────────────┐      ┌───────────────────┐  │   │       pavctl       │      └────────▲─────────┘
│ Static Files │─────▶│ pavis-ingest-file │─────▶│   pavis-codec-*   │──┘   │   (Tool / CLI)     │──────(debug)──┘
└──────────────┘      └───────────────────┘      └───────────────────┘      └────────────────────┘
                                                
```

All arrows represent data or artifact flow, not call graphs or control flow.
Fixed dataflow: **Ingest → SourceArtifact → Codec → RuntimeConfig → Relay → PVS Artifact → Runtime**.
Type-level validation flow: **SourceArtifact → CheckedArtifact → RuntimeConfig → ValidatedRuntimeConfig → Relay**.

The project is structured as a workspace with strict module boundaries to enforce the separation of concerns.

### 1.2. Sidecar Scope (Outbound-first)

Pavis is **outbound-first**: it primarily targets service-to-service proxying (Linkerd-style).
Inbound use is possible but **optional/future** and tends to pull the runtime toward gateway/policy-engine concerns.
Inbound behavior must be treated as an explicit tradeoff and **must not** leak gateway-style policy logic into the runtime by default.

### 1.3. Terminology (Artifacts and Boundaries)

- **SourceArtifact**: raw source bytes + metadata emitted by ingest (this is `Artifact` in `pavis-ingest-api`).
- **PVS Artifact**: the `.pvs` binary produced from a validated `RuntimeConfig` by `pavis-pvs`.
- **Envelope** (deprecated): avoid this term; use **SourceArtifact** instead to keep the ingest → codec boundary explicit.

### 1.4. Layer Rationale (Why these boundaries exist)

- **Relay exists today** to integrate with existing Envoy ecosystems: it publishes versioned `.pvs` artifacts and distributes them without imposing governance.
- **Codec stays a logical boundary** even if later embedded inside governor: the DTO ↔ RuntimeConfig transformation remains pure and testable.

### 2.1. Components

| Component              | Description                                                                                       |
| :--------------------- | :------------------------------------------------------------------------------------------------ |
| **`pavis`**            | Proxy – Runtime Engine. Reads optimized `.pvs` binary files only.                                 |
| **`pavis-core`**       | Protocol – Canonical types, semantic validation, and memory layout.                               |
| **`pavis-relay`**      | Relay – Versions `.pvs`, manages caches/last-known-good, and distributes artifacts via long poll. |
| **`pavis-ingest-*`**   | Ingest – Source connectivity (xDS, K8s, file watch): streams, auth, retries, resync.              |
| **`pavis-ingest-api`** | Ingest API – SourceArtifact (raw bytes + metadata) and ingest trait boundary.                    |
| **`pavis-codec-*`**    | Codec – DTO ↔ RuntimeConfig transforms, mechanical defaults, compatibility, and core validation.  |
| **`pavis-codec-api`**  | Codec API – Codec trait boundary for SourceArtifact ↔ RuntimeConfig transforms.                  |
| **`pavis-pvs`**        | Binary Protocol – Integrity layer (Header + Checksum + Encoding).                                 |
| **`pavctl`**           | CLI – Developer tool for manual generation, conversion, and runtime management.                   |

**Crate naming guidance:**
- `pavis-ingest-istio`, `pavis-ingest-k8s`, `pavis-ingest-file`
- `pavis-codec-xds`, `pavis-codec-crd`, `pavis-codec-serde`

### 2.2. Dependency Graph

*   **`pavis-core` (Root)**: The foundation. Canonical types and semantic validation. No I/O.
*   **`pavis-pvs`**: Depends on `pavis-core`. Handles the binary lifecycle.
*   **`pavis-codec-api`**: Defines the Codec boundary. Depends on `pavis-core` and ingest SourceArtifact types.
*   **`pavis-ingest-api`**: Defines the Ingest boundary (SourceArtifact + metadata).
*   **`pavis-codec-*`**: Pure logic crates. Depend on `pavis-core` and `pavis-codec-api` for DTO ↔ RuntimeConfig mapping and semantic validation.
*   **`pavis-ingest-*`**: Connectivity crates. Handle I/O and transport to upstream sources; emit SourceArtifacts (bytes + metadata) only.
*   **`pavis-relay`**: Coordinates ingest/codec outputs, versions artifacts, and distributes `.pvs`. It does artifact-level header/payload handling only and does not decode DTOs or `RuntimeConfig`.
*   **`pavctl`**: Depends on codecs and `pavis-pvs` to provide manual control and local tooling.
*   **`pavis` (Runtime)**: Depends on `pavis-core` and `pavis-pvs` only. **Must not** depend on ingest/codec/relay/governor.

**Current implementation note:** The repository currently ships a relay that accepts **PVS Artifacts** via HTTP publish and long-poll distribution. The ingest/codec pipeline remains a control-plane concern and may be integrated later; relay must remain DTO-agnostic regardless.

### 2.3. Responsibilities

| Responsibility     | Component        | Description                                                                                        |
| :----------------- | :--------------- | :------------------------------------------------------------------------------------------------- |
| **Ingest**         | `pavis-ingest-*` | Subscribes to configuration sources. Handles auth, watch/stream, retries, and resync.              |
| **Codec**          | `pavis-codec-*`  | Maps raw source DTOs to `RuntimeConfig`, applies mechanical defaults, and invokes core validation. |
| **Relay**          | `pavis-relay`    | Versions artifacts, manages caches/last-known-good, and serves `.pvs` via long-poll.               |
| **Governor**       | `pavis-governor` | Admission, policy enforcement, and approval of change plans (future/optional).                     |
| **Manual Tooling** | `pavctl`         | Reuses codecs for local file generation (`gen`), conversion (`convert`), and manual `apply`.       |
| **Integrity**      | `pavis-pvs`      | Computes checksums and adds protocol headers to encoded payloads.                                  |
| **Execution**      | `pavis`          | Zero-copy execution of the binary config. No semantic knowledge of the source.                     |

## 3. Modular Ingest Pipeline

To support diverse environments (Kubernetes, Service Meshes, and standalone files), Pavis employs a decoupled ingest architecture coordinated by the **Relay**.

### 3.1. Roles and Responsibilities

1.  **pavis-ingest-* (The Connectivity Layer)**:
    *   **Responsibility**: Implements transport logic to upstream sources (gRPC streams, watches, auth, retries, reconnect, resync).
    *   **Output**: Emits **SourceArtifacts** (raw bytes + metadata) into the pipeline.

2.  **pavis-codec-* (The Transformation Layer)**:
    *   **Responsibility**: Converts SourceArtifacts (xDS, YAML, CRD, JSON bytes) into the canonical `RuntimeConfig` model and back (best-effort).
    *   **Purity**: Codecs are pure transformers; no I/O, no networking.
    *   **Validation**: Performs source-specific preflight validation (Artifact → CheckedArtifact), then invokes canonical semantic validation in `pavis-core` (RuntimeConfig → ValidatedRuntimeConfig).

3.  **pavis-relay (The Distribution Layer)**:
    *   **Responsibility**: Manages **PVS Artifacts** (versioning, checksums, cache/last-known-good) and distributes them via long polling.
    *   **Invariant**: Enforces the **Single Source Authority** execution-time constraint—only one approved source controls the proxy at a time.
    *   **Constraint**: The relay MUST NOT parse DTOs or decode `RuntimeConfig`, but it MUST handle PVS bytes. Integrity checks should be delegated to `pavis-pvs` (header validation, checksum, archive integrity), and the relay should cache and serve the verified bytes.
    *   **HTTP API Contract**: See `crates/pavis-relay/README.md` for the endpoint-level contract and long-poll semantics.

Pavis-relay exposes a small HTTP surface for distributing versioned `.pvs` artifacts to sidecars. It is a pure artifact distribution server that uses long-polling so sidecars can fetch new configs as they become available without push channels or schema knowledge. Relay operates on artifact bytes (header + payload) and never decodes the payload into `RuntimeConfig`.

Core endpoints:
- GET /v1/config
  - Purpose: Sidecar configuration fetch via long-poll.
  - Required request headers: X-Pavis-Version (current client version).
  - Behavior: If the relay version is newer, returns the active `.pvs` immediately. Otherwise holds the connection until a new version is published or a timeout occurs.
  - Responses:
    - 200 OK with `.pvs` bytes and headers X-Pavis-Version and X-Pavis-Checksum (payload checksum; header excluded).
    - 304 Not Modified (or 204 No Content if configured) on timeout.
- GET /v1/status
  - Purpose: Operational status and health.
  - Returns current active version, checksum, artifact size, uptime, and last update time.

Publish endpoint (early deployments):
- POST /v1/publish
  - Purpose: Publish a new `.pvs` artifact to the relay.
  - Request body: Raw `.pvs` bytes.
  - Relay responsibilities: Validate PVS header/payload integrity (magic, version, payload checksum), persist the artifact, update the active version, and wake long-polling clients.
  - The relay does not parse or interpret configuration semantics.

Optional operational endpoints:
- GET /v1/artifacts/{version} for debugging or rollback.
- GET /v1/metrics for Prometheus-style monitoring.

### 3.x. Relay Packaging and Build Strategy

Pavis-relay is a standalone binary and Docker image. It orchestrates ingest + codec pipelines and distributes `.pvs` artifacts over HTTP long-polling. Ingest crates handle upstream I/O, codec crates are pure transformations, and the relay itself never parses DTOs. Pipelines are compiled into the relay as static plugins, selected at runtime from the bootstrap YAML only if they were built into the image.

#### Release Strategies

**A) Full relay image (early-stage convenience)**
- One image includes all supported ingest + codec combinations compiled in.
- Pros: Simple to distribute, fewer artifacts to manage.
- Cons: Larger image, broader attack surface, less deterministic deployments.

**B) Minimal relay images (recommended for production)**
- Multiple images, each built with a specific ingest + codec pipeline.
- Pros: Smaller images, tighter supply chain, explicit deployment intent.
- Cons: More build artifacts and CI work.

#### Cargo Features: Compile-Time Pipeline Selection

Use Cargo features to include specific ingest + codec crates in `pavis-relay`. Each pipeline feature enables exactly one ingest + codec pair.

Example feature mapping (illustrative):
- `plugin-file-yaml` -> `pavis-ingest-file` + `pavis-codec-serde`
- `plugin-xds-xds` -> `pavis-ingest-xds` + `pavis-codec-xds`
- `plugin-k8s-crd` -> `pavis-ingest-k8s` + `pavis-codec-crd`
- `full` -> enables all supported `plugin-*` features

Relay startup behavior:
- The bootstrap YAML specifies the ingest + codec pipeline by name.
- On startup, the relay checks that the configured pipeline is compiled in.
- If the pipeline is not compiled in, startup fails fast with a clear error.

#### Docker Image Builds Per Pipeline

Build images by selecting Cargo features at build time:
- Full image: `--features full`
- Minimal image: `--features plugin-file-yaml` (or other curated pipeline)

Tag images by pipeline so operators can select the correct artifact:
- `pavis-relay:full`
- `pavis-relay:file-yaml`
- `pavis-relay:xds-xds`
- `pavis-relay:k8s-crd`

#### CI Strategy: Curated Build Matrix

CI should build only a small, curated set of official pipelines to avoid matrix explosion:
- Use a build matrix (GitHub Actions/GitLab CI) or `docker buildx bake`.
- Each matrix entry maps to one pipeline feature set and image tag.
- Keep the list short and explicit; new pipelines require explicit approval.

#### Runtime Selection and Guardrails

- The bootstrap YAML selects the ingest + codec pipeline at runtime.
- Only pipelines compiled into the image are valid.
- This preserves relay purity (no DTO parsing) and keeps deployments deterministic.

Design principles:
- Relay handles artifacts, not configuration semantics.
- Relay never processes DTOs.
- Relay enforces a single active version at distribution time.
- Runtime pulls configuration; relay never pushes.

## 4. pavctl: The Pavis CLI

`pavctl` is the primary control interface for developers and operators, providing both offline protocol tooling and online runtime management.

### 4.1. Command Categories

1.  **Binary Protocol Tooling**:
    *   **gen**: Compiles high-level configurations (YAML/xDS) into optimized `.pvs` payloads.
    *   **view**: Provides human-readable views of binary state and protocol headers.
    *   **check**: Ensures source configurations (YAML) are free from semantic errors.
    *   **convert**: Reconstructs source configurations from binary files.
    *   **visualize**: Generates a visual representation of the logical configuration structure.

2.  **Runtime Orchestration**:
    *   **status**: Displays the current health, uptime, and active configuration version of the runtime.
    *   **logs**: Streams real-time logs from proxy instances for troubleshooting.

3.  **Configuration Management**:
    *   **rollback**: Instant recovery by reverting to a previous configuration version.
    *   **simulate**: Predicts routing outcomes for a given configuration without impacting live traffic.

## 5. Proxy Runtime Architecture

The `pavis` crate has been refactored into a **Domain-Driven Architecture** with strict module boundaries and invariants.

### 5.1. Modules

1.  **Config (`config`)**:
    *   **Role**: Internal Runtime Configuration.
    *   **Invariant**: `pavis-core` is the canonical model and the runtime may depend on it directly. If a `config` module exists, it should be a thin wrapper (runtime-only fields like compiled regexes are OK) and must be loaded from `ValidatedRuntimeConfig` only.
    *   **Key Types**: `Config`, `VirtualHost`, `Upstream`.

2.  **Router (`router`)**:
    *   **Role**: Immutable request matching logic.
    *   **Invariant**: Deterministic matching order. Regexes are pre-compiled at initialization (never on the hot path).
    *   **Key Types**: `Router`, `matcher`.

3.  **Upstream Manager (`upstream`)**:
    *   **Role**: Ownership of backend clusters and mutable load balancing state.
    *   **Invariant**: Thread-safe endpoint selection. Atomic updates to state. False sharing mitigation via cache-line alignment (`AlignedCounter`).
    *   **Key Types**: `Manager`, `Cluster`, `AlignedCounter`.

4.  **Telemetry (`telemetry`)**:
    *   **Role**: Performance-oriented logging and metrics.
    *   **Invariant**: **Non-blocking**. Operations must never block the request path (use `try_send` or background tasks). Failures result in dropped data, not crashes.
    *   **Key Types**: `Telemetry`, `AccessLog`.

5.  **Proxy Service (`proxy`)**:
    *   **Role**: The "Thin Controller" that orchestrates the above modules.
    *   **Invariant**: No business logic. No blocking calls. No ownership of mutable global state.
    *   **Key Types**: `Proxy` (implements `Pingora::ProxyHttp`).

### 5.2. Data Flow

```
Request -> Proxy -> Router (Match) -> Upstream Manager (Select Endpoint) -> Cluster -> Endpoint
             │
             ▼
        Telemetry (Log)
```

## 6. The PVS Protocol

The core innovation of Pavis is the **PVS Protocol**, a zero-copy binary configuration format.

### 6.1. File Format

| Offset | Size | Type       | Value  | Description                                 |
| ------ | ---- | ---------- | ------ | ------------------------------------------- |
| `0x00` | 4    | `[u8; 4]`  | `PAVS` | Magic bytes – identifies file type          |
| `0x04` | 4    | `u32`      | `0`    | Version – schema version for compatibility  |
| `0x08` | 4    | `u32`      | `1`    | Algorithm – Hash Algorithm ID (1 = SHA-256) |
| `0x0C` | 32   | `[u8; 32]` | ...    | Checksum – SHA-256 hash of the payload      |
| `0x2C` | 20   | `[u8; 20]` | `0`    | Reserved – Future proofing                  |
| `0x40` | ...  | `bytes`    | ...    | Payload – the `ArchivedRuntimeConfig` root  |

### 6.2. Versioning Strategy

Pavis uses a simple monotonically increasing integer for the protocol version (`PAVIS_VERSION` in `pavis-pvs`).

*   **Breaking Changes**: Any change to the `RuntimeConfig` struct layout (fields, enums) requires incrementing `PAVIS_VERSION`. `rkyv` is sensitive to layout.
*   **Non-Breaking Changes**: Documentation or internal helper methods.

### 6.3. Migration & Compatibility

Pavis prioritizes speed and simplicity over complex in-place migrations, while keeping
compatibility logic in the control plane rather than the runtime.

**Compatibility validation (always on):**
*   Maintain versioned "golden" PVS fixtures (e.g., vN, vN-1).
*   New `pavis-pvs` builds must parse and validate headers of older fixtures.
*   CI runs compatibility checks against prior fixtures to detect breaking changes early.
*   Validation does not imply runtime execution; it makes failures explicit and diagnosable.

**Control-plane migration (relay/governor):**
*   `pavis-relay` may accept older PVS versions (N-1).
*   Relay validates older artifact headers/payloads and coordinates re-emission into the current
    protocol version via the ingest/codec control-plane path; it does not decode `RuntimeConfig`.
*   Offline tooling (e.g., `pavctl convert --from <old> --to <current>`) provides explicit migration.

**Runtime contract (strict):**
*   `pavis` only accepts current-version PVS artifacts.
*   Any version mismatch is a hard error; the runtime does not load or migrate older versions.

**Upgrade Path:**
1.  Upgrade `pavis-relay`.
2.  Roll proxies (`pavis`), which remain strict to the current protocol version.

### 6.4. Performance Benefits

1. **Minimized Parsing** – Pavis uses `mmap` to map the file into memory. *Note: Current implementation performs eager deserialization into owned DTOs for runtime safety. Future versions will move to true zero-copy access.*
2. **Lazy Loading (Planned)** – In future versions, if config contains 10,000 routes (50MB) but the app only calls 2 services, the OS will only load the specific 4KB pages needed. Currently, the entire config is loaded into the heap at startup.

### 6.5. Distribution (Long Polling)

Pavis avoids the complexity of gRPC bidirectional streams in the sidecar. It uses HTTP Long Polling.

```
pavis-proxy                               pavis-relay
     │                                       │
     │  GET /config                          │
     │  X-Pavis-Version: 105                 │
     │──────────────────────────────────────▶│
     │                                       │
     │           (holds connection           │
     │            until version 106)         │
     │                                       │
     │  200 OK                               │
     │  X-Pavis-Version: 106                 │
     │  X-Pavis-Checksum: <sha256>           │
     │◀──────────────────────────────────────│
     │                                       │
     ▼  verify payload checksum, write config.pvs   ▼
```

### 6.6. rkyv Usage Guidelines

**Purpose**: Safely load `.pvs` binary configs into domain objects (`RuntimeConfig`) while keeping runtime and core free from serialization details.

#### Layer Responsibilities

1.  **Core (`pavis-core`)**
    *   Define canonical domain structs and semantic validation of `RuntimeConfig`.
    *   Can use `#[with(...)]` for archive compatibility.
    *   **Do not** deserialize or handle `.pvs` files.

2.  **PVS Protocol Crate (`pavis-pvs`)**
    *   Read `.pvs` files (disk/network).
    *   Validate integrity only (magic bytes, version, checksum, `check_archived_root`).
    *   Deserialize rkyv to owned `RuntimeConfig`; surface version mismatch or corruption as errors.
    *   Write `.pvs` files from validated `RuntimeConfig` (mechanical encoding only).
    *   Return clean domain objects and header-only inspection helpers.
    *   **Do not** implement runtime logic or semantic validation.

3.  **Runtime (`pavis`)**
    *   Consume `RuntimeConfig` for business logic.
    *   Do not depend on rkyv, `.pvs` format, or codecs.
    *   Build runtime state; only minimal crash-safety guards, no parsing or semantic validation.

#### Adapters (`#[with(...)]`)
*   Only for fields that cannot archive directly (String, Vec<T>, Regex, etc.).
*   Never wrap the entire root or expose codecs in API.

#### Key Principles
1.  **Core**: define structure only.
2.  **PVS Protocol Crate**: digest binary format, provide clean domain object.
3.  **Runtime**: pure business logic, no serialization knowledge.

> **Note:** rkyv is a storage protocol, codecs are implementation details, `pavis-pvs` isolates complexity.

### 6.7. Core Structs (Reference)

Quick reference for serialized types. `RuntimeConfig` lives in `crates/pavis-core/src/runtime.rs`; the PVS header lives in `crates/pavis-pvs/src/header.rs`. Authoritative definitions remain in code.

#### `PvsHeader`
```
PvsHeader
├─ magic: [u8; 4]          // magic bytes "PAVS" to identify file type
├─ version: u32            // protocol version expected by binaries
├─ algorithm: u32          // checksum algorithm id (1 = SHA-256)
├─ checksum: [u8; 32]      // checksum over payload (header excluded)
└─ _reserved: [u8; 20]     // padding/reserved for future fields
```

#### `RuntimeConfig`
```
RuntimeConfig
├─ server: ServerConfig
│  ├─ listen_addr: SocketAddr             // IP:port to bind
│  ├─ worker_threads: Option<u64>         // worker count override
│  └─ tls: Option<TlsConfig>
│     ├─ enabled: bool                    // enable TLS listener
│     ├─ cert_path: Option<String>        // certificate path
│     └─ key_path: Option<String>         // private key path
├─ telemetry: TelemetryConfig
│  ├─ level: Option<LogLevel>             // log level enum (Error, Warn, Info, Debug, Trace)
│  ├─ pingora: Option<LogLevel>           // optional pingora log level
│  ├─ service_name: Option<String>        // service identifier
│  ├─ prometheus_addr: Option<String>     // metrics endpoint bind address
│  ├─ access_log: AccessLogConfig         // False | Stdout | File(path)
│  └─ tracing: Option<TracingConfig>
│     ├─ enabled: bool                    // tracing on/off
│     ├─ provider: String                 // tracing backend name
│     └─ sampling_rate: f64               // sampling rate (0.0–1.0)
├─ upstreams: Vec<Upstream>
│  ├─ name: String                        // cluster name
│  ├─ load_balancer: LoadBalancer         // RoundRobin | Random
│  ├─ http_version: HttpVersion           // H1 | H2 | H2H1
│  ├─ connection_pool: ConnectionPoolConfig
│  │  ├─ idle_timeout_secs: u64           // idle keepalive timeout
│  │  └─ connection_timeout_secs: u64     // connect timeout
│  ├─ tls: Option<UpstreamTlsConfig>
│  │  ├─ enabled: bool                    // enable upstream TLS
│  │  ├─ verify_hostname: bool            // enforce hostname verification
│  │  ├─ verify_cert: bool                // enforce certificate validation
│  │  └─ sni: Option<String>              // explicit SNI override
│  └─ endpoints: Vec<Endpoint>
│     ├─ ip: IpAddr                       // backend IP/hostname
│     ├─ port: u16                        // backend port
│     └─ weight: u32                      // load-balancing weight
└─ routes: Vec<VirtualHost>
   ├─ host: String                        // vhost match (e.g., example.com or *)
   └─ paths: Vec<Route>
      ├─ match_type: MatchType            // Prefix | Exact | Regex
      ├─ path: String                     // path pattern per match_type
      ├─ timeout_ms: Option<u64>          // per-route timeout
      ├─ retry_policy: Option<RetryPolicy>
      │  ├─ attempts: u32                 // retry attempts
      │  ├─ per_try_timeout_ms: u64       // timeout per try
      │  └─ retry_on: Vec<String>         // retry conditions
      ├─ request_headers: Option<HeaderOperations>
      │  ├─ add: Vec<(String, String)>    // headers to add
      │  └─ remove: Vec<String>           // headers to remove
      ├─ response_headers: Option<HeaderOperations>
      │  ├─ add: Vec<(String, String)>    // headers to add
      │  └─ remove: Vec<String>           // headers to remove
      ├─ destinations: Vec<WeightedDestination>
      │  ├─ upstream: String              // target upstream name
      │  └─ weight: u32                   // destination weight
```

## 7. Safety & Resilience

Pavis employs a multi-layered strategy to ensure configuration stability, correctness, and operational safety.

### 7.1. Validation Strategy

1.  **Core Semantic Validation (`pavis-core`)**:
    *   **Canonical Truth.** Defines absolute semantics of `RuntimeConfig`.
    *   Ensures structural correctness and cross-resource invariants (e.g., reference integrity, regex validity).
    *   Source-agnostic and mandatory for all config producers.

2.  **Preflight / Input Validation (`pavis-codec-*`)**:
    *   **Artifact-level checks.** Enforces constraints tied to the input source (schema, defaults, compatibility).
    *   Produces a **CheckedArtifact** only after successful preflight checks.
    *   Decode/compile is allowed only from CheckedArtifact.
    *   Must invoke `pavis-core::validate_runtime` for canonical semantic validation.
    *   Must not redefine or partially duplicate core semantics.

3.  **PVS Integrity Validation (`pavis-pvs`)**:
    *   **Binary Safety.** Validates magic bytes, protocol version, checksum, and `rkyv::check_archived_root`.
    *   Deserializes to owned `RuntimeConfig`; surfaces version mismatch/corruption as errors.
    *   Does not perform semantic validation or compensation.

4.  **Runtime Defensive Guards (`pavis`)**:
    *   **Operational Safety.** Assumes semantically valid config; may add crash-safety guards.
    *   Applies runtime policy for version mismatches (startup fail vs hot-reload rejection).
    *   No parsing, decoding, or semantic validation.

> **Principle:** Semantics live in `pavis-core`; codecs produce validated configs; `pavis-pvs` guarantees binary integrity; runtime executes with minimal defensive guards.

**Validated types:** Codecs must produce CheckedArtifact and then ValidatedRuntimeConfig before Relay accepts input. Relay MUST NOT accept raw Artifact or unvalidated RuntimeConfig.

**Runtime input constraint:** The runtime sidecar MUST accept only `ValidatedRuntimeConfig` (via `pavis-pvs`), and MUST NOT attempt semantic validation.
**`ValidatedRuntimeConfig::from_trusted` usage:** Only construct via this helper when the config is already validated by the control plane or codec pipeline; it is not a substitute for semantic validation.

**RuntimeConfig validation entrypoint:** Producers/codecs must call `pavis-core::validate_runtime` after adaptation and before serialization. The CLI should rely on the codec conversion pipeline to invoke canonical validation; the runtime must not call it during startup or hot-reload.

### 7.1.1. Boundary Non-Goals

- **Codec MUST NOT** perform I/O, networking, filesystem access, or governance decisions.
- **Relay MUST NOT** parse DTO schemas or perform source-specific decoding.
- **Runtime MUST NOT** interpret upstream protocols or perform semantic validation.

### 7.1.2. Validation Layer Summary

| Layer                          | MUST validate                                                                    | MUST NOT validate                        | Owner           | Trigger point                            |
| :----------------------------- | :------------------------------------------------------------------------------- | :--------------------------------------- | :-------------- | :--------------------------------------- |
| **Artifact-level (preflight)** | Input shape, schema, format version, feature gates, source-specific constraints  | Canonical semantics, binary integrity    | `pavis-codec-*` | `Artifact → CheckedArtifact`             |
| **Canonical semantic**         | Cross-resource consistency, referential integrity, canonical defaults/invariants | Input schema/syntax, binary integrity    | `pavis-core`    | `RuntimeConfig → ValidatedRuntimeConfig` |
| **Binary integrity**           | Magic bytes, protocol version, checksum, archive integrity                       | Input schema/syntax, canonical semantics | `pavis-pvs`     | `PVS → RuntimeConfig`                    |

### 7.1.3. Error Taxonomy and Ownership

| Error class                      | Constructed by   | Notes                                                  |
| :------------------------------- | :--------------- | :----------------------------------------------------- |
| **Input / schema / syntax**      | `pavis-codec-*`  | User-correctable input errors                          |
| **Compatibility / feature-gate** | `pavis-codec-*`  | User-correctable incompatibilities                     |
| **Canonical semantic**           | `pavis-core`     | Canonical invariants; must not be fabricated elsewhere |
| **Binary integrity**             | `pavis-pvs`      | Integrity/compatibility failures                       |
| **Internal / invariant**         | Local layer only | Signals bugs or violated assumptions                   |

### 7.1.4. Error Propagation Rules

- **Codec** MUST pass through `pavis-core` errors as a distinct semantic variant; MUST NOT reinterpret them as input errors.
- **Relay** MAY summarize for API responses but MUST preserve original error details for logs/diagnostics.
- **Runtime** MUST surface `pavis-pvs` errors distinctly and MUST NOT attempt semantic validation.

### 7.2. Crash-Loop Protection

- Configuration persisted to disk (`/etc/pavis/config.pvs`)
- If Control Plane is down during Pod restart, Pavis loads last known good config and serves traffic immediately

### 7.3. Strategic Filtering

To prevent "Config Bloat" (a major issue in Envoy), **filtering is a control-plane responsibility**.
It must happen **before** `.pvs` emission, in the codec/governor path, not inside the relay.

- **Network Efficiency** – Only emit routes relevant to the target workload (Namespace, SidecarScope, or policy).
- **Security** – A compromised sidecar only receives IPs it is explicitly allowed to talk to.

Relay remains a byte-level distributor and MUST NOT decode `RuntimeConfig` to perform filtering.

## Future: Governor (Control Plane)

Governor is not present in early deployments; external control planes (Istio/Kuma) remain authoritative initially.
It is introduced only when Pavis evolves toward a first-party control plane that replaces external authorities.

When introduced, Governor sits above `pavis-relay` to separate *authorization* from *execution*.
It is the gatekeeper for **which codec are allowed**, **which policies must be enforced**,
and **which releases are approved** before any `.pvs` is produced or activated.

Key responsibilities:
- **Admission & policy enforcement** (tenant, environment, limits, rollout rules).
- **Plan approval**: turns a requested change into an approved, auditable plan.
- **Rollout/rollback decisions** with auditability, without embedding a reconciliation loop.

What it does *not* do:
- It does not parse or validate schemas (still done in codecs and `pavis-core`).
- It does not modify runtime behavior (runtime is unaware of governance).

When introduced, Governor sits above `pavis-relay`, which becomes an execution engine for **pre-approved plans**.
`pavis-core` remains the single source of canonical semantic validation.

Governor is optional in early deployments and not required while Pavis relays existing Envoy ecosystems. It becomes mandatory as multi-tenant policy and audit requirements mature.

```text
Governor ──▶ pavis-relay (Execution) ──▶ pavis-pvs ──▶ pavis (Runtime)
```

When Governor is present, `pavctl apply` becomes a **release request** subject to approval.
Governor can approve, defer, or reject, and only approved plans are executed by `pavis-relay`.

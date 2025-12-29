# Architecture

## 1. Architecture Overview

Pavis replaces the traditional "Smart Proxy" model (Envoy) with a **Split Data Plane** architecture. Heavy lifting like parsing, defaulting, and semantic validation is offloaded to a centralized bridge, keeping the sidecar proxy lightweight and binary-focused.

### 1.1. End-to-End Configuration Flow

```text
  Source Type          Connectivity (Feeds)     Translation (Adapters)        Management & Tools            Data Plane
┌──────────────┐      ┌──────────────────┐      ┌────────────────────┐      ┌────────────────────┐      ┌──────────────────┐
│ Mesh (Istio) │─────▶│ pavis-feed-istio │──┐   │ pavis-adapter-xds  │──┐   │   pavis-manager    │─────▶│    pavis-pvs     │
└──────────────┘      └──────────────────┘  ├──▶│ (Protobuf → Core)  │  │   │  (Orchestration)   │  ┌──▶│ (Binary Protocol)│
┌──────────────┐      ┌──────────────────┐  │   └────────────────────┘  ├──▶└──────────▲─────────┘  │   └──────────────────┘
│ Mesh (Kuma)  │─────▶│ pavis-feed-kuma  │──┘                           │              │            │             
└──────────────┘      └──────────────────┘      ┌────────────────────┐  │              │            │             
┌──────────────┐      ┌──────────────────┐      │ pavis-adapter-crd  │──┘              │ (apply)    │   ┌──────────────────┐
│ Kubernetes   │─────▶│ pavis-feed-k8s   │─────▶│ (K8s Gateway→Core) │                 │            └──▶│      pavis       │
└──────────────┘      └──────────────────┘      └────────────────────┘      ┌──────────┴─────────┐      │    (Runtime)     │
                                                                            │       pavctl       │─────▶└────────▲─────────┘
┌──────────────┐                     ┌────────────────────┐                 │   (Tool / CLI)     │               │
│ Static Files │◀───────────────────▶│  pavis-adapter-*   │◀───────────────▶│                    │──────(debug)──┘
└──────────────┘                     │    (DTO → Core)    │                 └────────────────────┘
                                     └────────────────────┘
```

The project is structured as a workspace with strict module boundaries to enforce the separation of concerns.

### 1.2. Governance & Control Plane

Pavis adds a lightweight Governance / Control layer above `pavis-manager` to separate *authorization* from *execution*.
This layer is the gatekeeper for **which feed+adapter combinations are allowed**, **which policies must be enforced**,
and **which releases are approved** before any `.pvs` is produced or activated.

Key responsibilities:
- **Admission & policy enforcement** (tenant, environment, limits, rollout rules).
- **Plan approval**: turns a requested change into an approved, auditable plan.
- **Rollout/rollback decisions** with auditability, without embedding a reconciliation loop.

What it does *not* do:
- It does not parse or validate schemas (still done in adapters and `pavis-core`).
- It does not modify runtime behavior (runtime is unaware of governance).

Governance sits above `pavis-manager`, which becomes an execution engine for **pre-approved plans**.
`pavis-core` remains the single source of canonical semantic validation.

```text
Governance ──▶ pavis-manager (Execution) ──▶ pavis-pvs ──▶ pavis (Runtime)
```

`pavctl apply` is treated as a **release request**, not an immediate activation.
Governance can approve, defer, or reject, and only approved plans are executed by `pavis-manager`.

### 2.1. Components

| Component | Description |
| :--- | :--- |
| **`pavis`** | Proxy – Runtime Engine. Reads optimized `.pvs` binary files only. |
| **`pavis-core`** | Protocol – Canonical types, shared `Config` trait, and memory layout. |
| **`pavis-manager`** | Orchestrator – Coordinates loading, validation, and forwarding. Enforces single-source authority. |
| **`pavis-feed-*`** | Connectivity – Source-specific connectors (Istio, Kuma, K8s). |
| **`pavis-adapter-*`** | Translation – Protocol-specific mappers (xDS, YAML, CRD) to the canonical model. |
| **`pavis-pvs`** | Binary Protocol – Integrity layer (Header + Checksum + Encoding). |
| **`pavctl`** | CLI – Developer tool for manual generation, conversion, and runtime management. |

### 2.2. Dependency Graph

*   **`pavis-core` (Root)**: The foundation. Defines the shared `Config` trait. No I/O.
*   **`pavis-pvs`**: Depends on `pavis-core`. Handles the binary lifecycle.
*   **`pavis-adapter-*`**: Pure logic crates. Depend on `pavis-core` to map external protocols to `RuntimeConfig`.
*   **`pavis-feed-*`**: Connectivity crates. Handle I/O and transport.
*   **`pavis-manager`**: Depends on feeds and adapters to orchestrate the automated pipeline.
*   **`pavctl`**: Depends on adapters and `pavis-pvs` to provide manual control and local tooling.
*   **`pavis` (Runtime)**: Depends on `pavis-core` and `pavis-pvs`. **Must not** depend on feeds or adapters.

### 2.3. Responsibilities

| Responsibility | Component | Description |
| :--- | :--- | :--- |
| **Connectivity** | `pavis-feed-*` | Subscribes to configuration sources. Handles auth and networking. |
| **Protocol Translation** | `pavis-adapter-*` | Maps raw source data (e.g., Envoy Protobuf) to `RuntimeConfig`. |
| **Orchestration** | `pavis-manager` | Picks **one** active Feed/Adapter pair. Validates results and emits `.pvs`. |
| **Manual Tooling** | `pavctl` | Reuses adapters for local file generation (`gen`), conversion (`convert`), and manual `apply`. |
| **Integrity** | `pavis-pvs` | Computes checksums and adds protocol headers to encoded payloads. |
| **Execution** | `pavis` | Zero-copy execution of the binary config. No semantic knowledge of the source. |

## 3. Modular Ingestion Pipeline

To support diverse environments (Kubernetes, Service Meshes, and standalone files), Pavis employs a decoupled ingestion architecture coordinated by the **Manager**.

### 3.1. Roles and Responsibilities

1.  **pavis-feed-* (The Connectivity Layer)**:
    *   **Responsibility**: Implements the transport logic to talk to external sources (e.g., Istiod gRPC, Kubernetes API).
    *   **Output**: Emits raw, unparsed configuration events into the pipeline.

2.  **pavis-adapter-* (The Translation Layer)**:
    *   **Responsibility**: Encapsulates the logic to map a specific protocol (xDS, YAML, CRD) into the Pavis canonical `RuntimeConfig` model.
    *   **Purity**: Adapters are pure transformers; they do not perform I/O or networking.

3.  **pavis-manager (The Orchestrator)**:
    *   **Responsibility**: Coordinates one active Feed/Adapter pair. It receives translated configurations, executes the core semantic validation gate, increments the protocol version, and emits the final `.pvs` payload via `pavis-pvs`.
    *   **Authority**: Enforces the **Single Source Authority** rule—only one configuration source can control the proxy at a time.

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
    *   **apply**: Dynamically pushes configuration to active proxy instances via the xDS bridge.
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
    *   **Invariant**: Decoupled from `pavis-core` types where appropriate. Loaded from `RuntimeConfig`.
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

| Offset | Size | Type | Value | Description |
|--------|------|------|-------|-------------|
| `0x00` | 4 | `[u8; 4]` | `PAVS` | Magic bytes – identifies file type |
| `0x04` | 4 | `u32` | `0` | Version – schema version for compatibility |
| `0x08` | 4 | `u32` | `1` | Algorithm – Hash Algorithm ID (1 = SHA-256) |
| `0x0C` | 32 | `[u8; 32]` | ... | Checksum – SHA-256 hash of the payload |
| `0x2C` | 20 | `[u8; 20]` | `0` | Reserved – Future proofing |
| `0x40` | ... | `bytes` | ... | Payload – the `ArchivedRuntimeConfig` root |

### 6.2. Versioning Strategy

Pavis uses a simple monotonically increasing integer for the protocol version (`PAVIS_VERSION` in `pavis-pvs`).

*   **Breaking Changes**: Any change to the `RuntimeConfig` struct layout (fields, enums) requires incrementing `PAVIS_VERSION`. `rkyv` is sensitive to layout.
*   **Non-Breaking Changes**: Documentation or internal helper methods.

### 6.3. Migration & Compatibility

Pavis prioritizes speed and simplicity over complex in-place migrations.

*   **Bridge-Side (`pavis-manager`)**: Translates user intent (YAML/xDS) to the current protocol version. Must be redeployed when protocol changes.
*   **Proxy-Side (`pavis`)**: Performs strict version checking.
    *   **Magic Bytes**: Must be `PAVS`.
    *   **Version**: Must match exactly. `pavis-pvs` surfaces mismatch as an error; the runtime owns the policy (startup fail vs hot-reload rejection).
*   **Upgrade Path**:
    1.  Upgrade Control Plane (`pavis-manager`).
    2.  Rolling update of Proxies (`pavis`).
    3.  No N-1 compatibility support currently.
*   **Future Improvements**:
    *   Schema Reflection or FlatBuffers if N-1 compatibility becomes required.
    *   `pavctl convert` tool for offline migration.

### 6.4. Performance Benefits

1. **Minimized Parsing** – Pavis uses `mmap` to map the file into memory. *Note: Current implementation performs eager deserialization into owned DTOs for runtime safety. Future versions will move to true zero-copy access.*
2. **Lazy Loading (Planned)** – In future versions, if config contains 10,000 routes (50MB) but the app only calls 2 services, the OS will only load the specific 4KB pages needed. Currently, the entire config is loaded into the heap at startup.

### 6.5. Distribution (Long Polling)

Pavis avoids the complexity of gRPC bidirectional streams in the sidecar. It uses HTTP Long Polling.

```
pavis-proxy                              pavis-manager
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
     │  X-Pavis-Checksum: <xxhash>           │
     │◀──────────────────────────────────────│
     │                                       │
     ▼  verify checksum, write config.pvs   ▼
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
    *   Do not depend on rkyv, `.pvs` format, or adapters.
    *   Build runtime state; only minimal crash-safety guards, no parsing or semantic validation.

#### Adapters (`#[with(...)]`)
*   Only for fields that cannot archive directly (String, Vec<T>, Regex, etc.).
*   Never wrap the entire root or expose adapters in API.

#### Key Principles
1.  **Core**: define structure only.
2.  **PVS Protocol Crate**: digest binary format, provide clean domain object.
3.  **Runtime**: pure business logic, no serialization knowledge.

> **Note:** rkyv is a storage protocol, adapters are implementation details, `pavis-pvs` isolates complexity.

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
      └─ compiled_regex: Option<regex::Regex>  // precompiled regex; runtime only
```

## 7. Safety & Resilience

Pavis employs a multi-layered strategy to ensure configuration stability, correctness, and operational safety.

### 7.1. Validation Strategy

1.  **Core Semantic Validation (`pavis-core`)**:
    *   **Canonical Truth.** Defines absolute semantics of `RuntimeConfig`.
    *   Ensures structural correctness and cross-resource invariants (e.g., reference integrity, regex validity).
    *   Source-agnostic and mandatory for all config producers.

2.  **Source-Specific Validation (`pavctl`, `pavis-manager`, adapters)**:
    *   **Input Adaptation.** Enforces constraints tied to the input source (schema, defaults, compatibility).
    *   Transforms user intent into `RuntimeConfig` and invokes `pavis-core::validate_runtime`.
    *   May not redefine or partially duplicate core semantics.

3.  **PVS Integrity Validation (`pavis-pvs`)**:
    *   **Binary Safety.** Validates magic bytes, protocol version, checksum, and `rkyv::check_archived_root`.
    *   Deserializes to owned `RuntimeConfig`; surfaces version mismatch/corruption as errors.
    *   Does not perform semantic validation or compensation.

4.  **Runtime Defensive Guards (`pavis`)**:
    *   **Operational Safety.** Assumes semantically valid config; may add crash-safety guards.
    *   Applies runtime policy for version mismatches (startup fail vs hot-reload rejection).
    *   No parsing, decoding, or semantic validation.

> **Principle:** Semantics live in `pavis-core`; adapters produce validated configs; `pavis-pvs` guarantees binary integrity; runtime executes with minimal defensive guards.

**RuntimeConfig validation entrypoint:** Producers/adapters must call `pavis-core::validate_runtime` after adaptation and before serialization. The CLI should rely on the adapter’s conversion pipeline to invoke canonical validation; the runtime must not call it during startup or hot-reload.

### 7.2. Crash-Loop Protection

- Configuration persisted to disk (`/etc/pavis/config.pvs`)
- If Control Plane is down during Pod restart, Pavis loads last known good config and serves traffic immediately

### 7.3. Strategic Filtering

To prevent "Config Bloat" (a major issue in Envoy), the Bridge (`pavis-manager`) performs aggressive filtering before compiling the `.pvs` file.

- **Network Efficiency** – Only sends routes relevant to the specific Pod (based on Namespace or SidecarScope)
- **Security** – A compromised sidecar only knows the IP addresses of services it is explicitly allowed to talk to

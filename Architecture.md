# Architecture

## 1. Overview

Pavis replaces the traditional "Smart Proxy" model (Envoy) with a **Split Data Plane** architecture. Instead of every sidecar performing expensive parsing, Pavis offloads complexity to a centralized bridge, keeping the sidecar lightweight and fast.

```
┌──────────────┐      ┌──────────────┐       ┌──────────────┐
│   Istiod     │      │  pavis-xds   │       │    pavis     │
│ (Control Pl) │─xDS─▶│   (Bridge)   │─HTTP─▶│   (Proxy)    │
└──────────────┘      └──────────────┘       └──────────────┘
                             │                      │
                             │    .pvs file         │
                             └──────────────────────┘
```

## 2. System Design & Boundaries

The project is structured as a workspace with strict module boundaries to enforce the separation of concerns between protocol, producers, and runtime.

### 2.1. Components

```
pavis/
├── crates/
│   ├── pavis/              # Proxy – Runtime Engine (Reads .pvs only)
│   ├── pavis-core/         # Protocol – Canonical types & memory layout
│   ├── pavis-adapter-yaml/ # Adapter – YAML Input DTOs, parsing, validation
│   ├── pavis-cli/          # CLI – I/O shell for local config compilation
│   └── pavis-xds/          # Bridge – I/O shell for xDS streams
└── Cargo.toml              # Workspace configuration
```

### 2.2. Dependency Graph

*   **`pavis-core` (Root)**: The foundation. Depends on `rkyv`. No I/O, no Serde.
*   **`pavis-adapter-yaml`**: Depends on `pavis-core` and input libs (`serde`, `yaml`).
*   **`pavis-cli` / `pavis-xds`**: Depend on `pavis-adapter-yaml` (or other adapters).
*   **`pavis` (Runtime)**: Depends on `pavis-core`. **Must not** depend on adapters or input libs.

### 2.3. Responsibilities

| Responsibility | Component | Description |
| :--- | :--- | :--- |
| **Protocol Definition** | `pavis-core` | Defines `.pvs` binary format and optimized `RuntimeConfig`. |
| **Input DTOs** | `pavis-adapter-*` | Defines `YamlConfig`, `XdsConfig` optimized for UX/Defaults. |
| **Adaptation & Validation** | `pavis-adapter-*` | Source-specific defaults/compat cleanup. Transforms Input DTO -> `RuntimeConfig` and invokes core semantic validation. |
| **I/O & Orchestration** | Producers | `cli` & `xds` read source configs/streams and invoke adapters to produce `.pvs`; inspection of `.pvs` may be performed by tooling but must not redefine semantics (integrity and version checks only). |
| **Runtime Execution** | `pavis` | Consumes validated `RuntimeConfig`; builds router/upstream/telemetry state. No parsing, decoding, or semantic validation of config. |

### 2.4. Layering Principles

1.  **Protocol Definition Layer (`pavis-core`)**
    *   Defines only the protocol and semantics.
    *   Includes `WireConfig`, `ArchivedWireConfig`, and canonical validation.
    *   No dependency on YAML, CLI, or runtime.
    *   Does not perform format conversion or legacy compatibility.

2.  **Boundary Layer (`pvs` crate)**
    *   Handles loading and unpacking.
    *   Owns binary/integrity validation: magic bytes, protocol version, checksum, and `rkyv::check_archived_root`.
    *   Deserializes to `RuntimeConfig` and returns clean domain objects; no semantic validation or compensation.
    *   Exposes a safe API to the runtime without leaking archive internals.

3.  **Runtime Layer (`pavis`)**
    *   Consumes only semantically validated configuration.
    *   Does not know about `rkyv`, `mmap`, or archive details.
    *   Builds runtime state (router, upstream manager, telemetry) and may perform crash-safety guards; no parsing/semantic validation.

> **Rule:** The Runtime never sees invalid or partial state. It relies on the Adapter to produce a valid `RuntimeConfig`.

### 2.5. Module Layout

*   **No `mod.rs`**. Use `<module>.rs` with submodules in `<module>/`.
*   Keep `<module>.rs` focused on module structure and re-exports; put logic in sibling files.
*   Split by responsibility (types vs logic vs I/O), but avoid over-splitting by size alone.

## 3. The PVS Protocol

The core innovation of Pavis is the **PVS Protocol**, a zero-copy binary configuration format.

### 3.1. File Format

| Offset | Size | Type | Value | Description |
|--------|------|------|-------|-------------|
| `0x00` | 4 | `[u8; 4]` | `PAVS` | Magic bytes – identifies file type |
| `0x04` | 4 | `u32` | `0` | Version – schema version for compatibility |
| `0x08` | 4 | `u32` | `1` | Algorithm – Hash Algorithm ID (1 = SHA-256) |
| `0x0C` | 32 | `[u8; 32]` | ... | Checksum – SHA-256 hash of the payload |
| `0x2C` | 20 | `[u8; 20]` | `0` | Reserved – Future proofing |
| `0x40` | ... | `bytes` | ... | Payload – the `ArchivedRuntimeConfig` root |

### 3.2. Versioning Strategy

Pavis uses a simple monotonically increasing integer for the protocol version (`PAVIS_VERSION` in `pavis-core`).

*   **Breaking Changes**: Any change to the `RuntimeConfig` struct layout (fields, enums) requires incrementing `PAVIS_VERSION`. `rkyv` is sensitive to layout.
*   **Non-Breaking Changes**: Documentation or internal helper methods.

### 3.3. Migration & Compatibility

Pavis prioritizes speed and simplicity over complex in-place migrations.

*   **Bridge-Side (`pavis-xds`)**: Translates user intent (YAML/xDS) to the current protocol version. Must be redeployed when protocol changes.
*   **Proxy-Side (`pavis`)**: Performs strict version checking.
    *   **Magic Bytes**: Must be `PAVS`.
    *   **Version**: Must match exactly. Boundary surfaces mismatch as an error; the runtime owns the policy (startup fail vs hot-reload rejection).
*   **Upgrade Path**:
    1.  Upgrade Control Plane (`pavis-xds`).
    2.  Rolling update of Proxies (`pavis`).
    3.  No N-1 compatibility support currently.
*   **Future Improvements**:
    *   Schema Reflection or FlatBuffers if N-1 compatibility becomes required.
    *   `pavis-cli convert` tool for offline migration.

### 3.4. Performance Benefits

1. **Minimized Parsing** – Pavis uses `mmap` to map the file into memory. *Note: Current implementation performs eager deserialization into owned DTOs for runtime safety. Future versions will move to true zero-copy access.*
2. **Lazy Loading (Planned)** – In future versions, if config contains 10,000 routes (50MB) but the app only calls 2 services, the OS will only load the specific 4KB pages needed. Currently, the entire config is loaded into the heap at startup.

### 3.5. Distribution (Long Polling)

Pavis avoids the complexity of gRPC bidirectional streams in the sidecar. It uses HTTP Long Polling.

```
pavis-proxy                              pavis-xds
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

### 3.6. rkyv Usage Guidelines

**Purpose**: Safely load `.pvs` binary configs into domain objects (`RuntimeConfig`) while keeping runtime and core free from serialization details.

#### Layer Responsibilities

1.  **Core (`pavis-core`)**
    *   Define protocol structs, magic/version constants, and canonical semantic validation of `RuntimeConfig`.
    *   Can use `#[with(...)]` for archive compatibility.
    *   **Do not** deserialize or handle `.pvs` files.

2.  **Boundary (`pavis/src/load`)**
    *   Read `.pvs` files.
    *   Validate integrity only (magic bytes, version, checksum, `check_archived_root`).
    *   Deserialize rkyv to owned `RuntimeConfig`; surface version mismatch or corruption as errors.
    *   Return clean domain objects.
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
2.  **Boundary**: digest binary format, provide clean domain object.
3.  **Runtime**: pure business logic, no serialization knowledge.

> **Note:** rkyv is a storage protocol, adapters are implementation details, Boundary layer isolates complexity.

## 4. Proxy Runtime Architecture

The `pavis` crate has been refactored into a **Domain-Driven Architecture** with strict module boundaries and invariants.

### 4.1. Modules

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

### 4.2. Data Flow

```
Request -> Proxy -> Router (Match) -> Upstream Manager (Select Endpoint) -> Cluster -> Endpoint
             │
             ▼
        Telemetry (Log)
```

## 5. Safety & Resilience

Pavis employs a multi-layered strategy to ensure configuration stability, correctness, and operational safety.

### 5.1. Validation Strategy

1.  **Core Semantic Validation (`pavis-core`)**:
    *   **Canonical Truth.** Defines absolute semantics of `RuntimeConfig`.
    *   Ensures structural correctness and cross-resource invariants (e.g., reference integrity, regex validity).
    *   Source-agnostic and mandatory for all config producers.

2.  **Source-Specific Validation (`pavis-cli`, `pavis-xds`, adapters)**:
    *   **Input Adaptation.** Enforces constraints tied to the input source (schema, defaults, compatibility).
    *   Transforms user intent into `RuntimeConfig` and invokes `pavis-core::validate_runtime_config`.
    *   May not redefine or partially duplicate core semantics.

3.  **Boundary Integrity Validation (`pvs`)**:
    *   **Binary Safety.** Validates magic bytes, protocol version, checksum, and `rkyv::check_archived_root`.
    *   Deserializes to owned `RuntimeConfig`; surfaces version mismatch/corruption as errors.
    *   Does not perform semantic validation or compensation.

4.  **Runtime Defensive Guards (`pavis`)**:
    *   **Operational Safety.** Assumes semantically valid config; may add crash-safety guards.
    *   Applies runtime policy for version mismatches (startup fail vs hot-reload rejection).
    *   No parsing, decoding, or semantic validation.

> **Principle:** Semantics live in `pavis-core`; adapters produce validated configs; boundary guarantees binary integrity; runtime executes with minimal defensive guards.

**RuntimeConfig validation entrypoint:** Producers/adapters must call `pavis-core::validate_runtime_config` after adaptation and before serialization. The CLI should rely on the adapter’s conversion pipeline to invoke canonical validation; the runtime must not call it during startup or hot-reload.

### 5.2. Crash-Loop Protection

- Configuration persisted to disk (`/etc/pavis/config.pvs`)
- If Control Plane is down during Pod restart, Pavis loads last known good config and serves traffic immediately

### 5.3. Strategic Filtering

To prevent "Config Bloat" (a major issue in Envoy), the Bridge (`pavis-xds`) performs aggressive filtering before compiling the `.pvs` file.

- **Network Efficiency** – Only sends routes relevant to the specific Pod (based on Namespace or SidecarScope)
- **Security** – A compromised sidecar only knows the IP addresses of services it is explicitly allowed to talk to

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
| **`pavis`**            | Proxy – Runtime Engine. Reads optimized `.pvs` binary files only. Executes fully prepared configs.|
| **`pavis-core`**       | Protocol – Canonical types, semantic validation, and memory layout.                               |
| **`pavis-relay`**      | Relay – Versions `.pvs`, manages caches. **Pass-through** for validation; distributes artifacts.  |
| **`pavis-ingest-*`**   | Ingest – Source connectivity (xDS, K8s, file watch): streams, auth, retries, resync.              |
| **`pavis-ingest-api`** | Ingest API – SourceArtifact (raw bytes + metadata) and ingest trait boundary.                     |
| **`pavis-codec-*`**    | Codec – DTO ↔ RuntimeConfig transforms, **default population**, and core validation.              |
| **`pavis-codec-api`**  | Codec API – Codec trait boundary for SourceArtifact ↔ RuntimeConfig transforms.                   |
| **`pavis-pvs`**        | Binary Protocol – Integrity layer (Header + Checksum + Encoding).                                 |
| **`pavctl`**           | CLI – Developer tool for manual generation, conversion, and runtime management.                   |

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

### 2.3. Responsibilities

| Responsibility     | Component        | Description                                                                                        |
| :----------------- | :--------------- | :------------------------------------------------------------------------------------------------- |
| **Ingest**         | `pavis-ingest-*` | Subscribes to configuration sources. Handles auth, watch/stream, retries, and resync.              |
| **Codec**          | `pavis-codec-*`  | Maps source DTOs to `RuntimeConfig`. **Populates defaults** and **validates** configuration.       |
| **Relay**          | `pavis-relay`    | Versions artifacts and distributes `.pvs`. **Pass-through** for logic; no validation or population.|
| **Governor**       | `pavis-governor` | Admission, policy enforcement, and approval of change plans (future/optional).                     |
| **Manual Tooling** | `pavctl`         | Reuses codecs for local file generation (`gen`), conversion (`convert`), and manual `apply`.       |
| **Integrity**      | `pavis-pvs`      | Computes checksums and adds protocol headers to encoded payloads.                                  |
| **Execution**      | `pavis`          | Zero-copy execution of the binary config. **No validation or default population**.                 |

## 3. Modular Ingest Pipeline

To support diverse environments (Kubernetes, Service Meshes, and standalone files), Pavis employs a decoupled ingest architecture coordinated by the **Relay**.

See [docs/specs/RELAY_PROTOCOL.md](docs/specs/RELAY_PROTOCOL.md) for protocol details and [docs/reference/API_RELAY.md](docs/reference/API_RELAY.md) for the HTTP contract.

### 3.1. Roles and Responsibilities

1.  **pavis-ingest-* (The Connectivity Layer)**: Implements transport logic to upstream sources.
2.  **pavis-codec-* (The Transformation Layer)**: Converts SourceArtifacts into `RuntimeConfig`. Populates defaults.
3.  **pavis-relay (The Distribution Layer)**: Manages **PVS Artifacts** (versioning, checksums) and distributes them.

## 4. pavctl: The Pavis CLI

`pavctl` is the primary control interface for developers and operators, providing both offline protocol tooling and online runtime management.

## 5. Proxy Runtime Architecture

The `pavis` crate has been refactored into a **Domain-Driven Architecture** with strict module boundaries and invariants.

See [docs/specs/RUNTIME_INTERNALS.md](docs/specs/RUNTIME_INTERNALS.md) for memory lifecycle and routing implementation details.

### 5.1. Modules

1.  **Config (`config`)**: Internal Runtime Configuration.
2.  **Router (`router`)**: Immutable request matching logic.
3.  **Upstream Manager (`upstream`)**: Ownership of backend clusters and mutable load balancing state.
4.  **Telemetry (`telemetry`)**: Performance-oriented logging and metrics.
5.  **Proxy Service (`proxy`)**: The "Thin Controller" that orchestrates the above modules.

### 5.2. Data Flow

```
Request -> Proxy -> Router (Match) -> Upstream Manager (Select Endpoint) -> Cluster -> Endpoint
             │
             ▼
        Telemetry (Log)
```

## 6. The PVS Protocol

The core innovation of Pavis is the **PVS Protocol**, a zero-copy binary configuration format.

See [docs/specs/PVS_BINARY_FORMAT.md](docs/specs/PVS_BINARY_FORMAT.md) for the byte-level specification.

### 6.1. File Format

The PVS file consists of a fixed-size header followed by an rkyv-archived payload.
The runtime maps this file directly into memory (`mmap`) for O(1) access.

## 7. Safety & Resilience

Pavis employs a multi-layered strategy to ensure configuration stability, correctness, and operational safety.

### 7.1. Validation Strategy

1.  **Core Semantic Validation (`pavis-core`)**: Canonical Truth.
2.  **Preflight / Input Validation (`pavis-codec-*`)**: Artifact-level checks.
3.  **PVS Integrity Validation (`pavis-pvs`)**: Binary Safety.
4.  **Runtime Defensive Guards (`pavis`)**: Operational Safety.

### 7.2. Crash-Loop Protection

- Configuration persisted to disk (`/etc/pavis/config.pvs`)
- If Control Plane is down during Pod restart, Pavis loads last known good config and serves traffic immediately

### 7.3. Strategic Filtering

To prevent "Config Bloat" (a major issue in Envoy), configuration is filtered and compacted during the codec transformation phase, before `.pvs` emission.

## 8. Configuration Architecture & Invariants

This section defines the Pavis configuration architecture, strictly separating **Policy** (Codec) from **Mechanism** (Runtime).

### 8.1. Architectural Invariants

The following rules are absolute.

#### Codec Layer (`pavis-codec-*`)
*   **Role:** Owner of **Defaults** and **Policy**.
*   **Responsibility:** MUST materialize all optional fields into explicit values.
*   **Responsibility:** MUST normalize user input (YAML/JSON/xDS) into `ValidatedRuntimeConfig`.
*   **Output:** A fully deterministic configuration. Implicit "magic" defaults MUST be resolved here.

#### Runtime Layer (`pavis`)
*   **Role:** Executor of **Explicit Configuration**.
*   **Input:** STRICTLY `ValidatedRuntimeConfig` (via `.pvs`).
*   **Invariant:** MUST NOT apply semantic defaults (e.g., "missing timeout means 5s"). `None` implies "Disabled" or "System Choice" (e.g., auto-threads), never "Business Default".
*   **Invariant:** MUST NOT mutate configuration after load.
*   **Failure:** MUST fail immediately on invalid structural state (e.g., missing cert files).

#### Relay Layer
*   **Role:** Distribution Pipeline.
*   **Invariant:** MUST treat `.pvs` blobs as opaque, immutable, and versioned.

#### .pvs Artifacts
*   **Nature:** Fully materialized and frozen.
*   **Guarantee:** A `.pvs` compiled today MUST execute identically on future runtime versions. Policy changes (defaults) affect only *newly compiled* artifacts.

### 8.2. Zero-Option Runtime

The Runtime consumes a configuration where all policy decisions are **explicit** and **materialized**. `Option<T>` fields are forbidden for policy configuration.

*   **Explicit State**: Optional features must be represented by explicit enums (e.g., `TlsMode::Disabled` vs `TlsMode::Enabled`) or empty collections.
*   **No Inference**: The Runtime MUST NOT infer defaults from missing values.
*   **Materialization**: The Codec is responsible for converting human-friendly optionality (YAML nulls) into concrete runtime states.
*   **Time Semantics**: Durations are materialized as `u32` milliseconds. `0` has specific, normative semantics per field (e.g., Infinite or Fail-Fast) and MUST NOT be reinterpreted as "default" by the runtime.

For a complete inventory of fields and their explicit semantics, see [docs/reference/CONFIGURATION.md](docs/reference/CONFIGURATION.md).

### 8.3. Codec Responsibility (Policy Ownership)

The **Codec Layer** is the sole owner of policy defaults. It transforms sparse user input into Fully Explicit `RuntimeConfig`.

### 8.4. User Configuration & References

Users provide minimal configuration (e.g., listeners and routes); the Codec expands this into the full Runtime state.

See [docs/reference/CONFIGURATION.md](docs/reference/CONFIGURATION.md) for the canonical definition of the Fully Explicit Runtime Configuration.

## Future: Governor (Control Plane)

Governor is not present in early deployments; external control planes (Istio/Kuma) remain authoritative initially.
It is introduced only when Pavis evolves toward a first-party control plane that replaces external authorities.

```text
Governor ──▶ pavis-relay (Execution) ──▶ pavis-pvs ──▶ pavis (Runtime)
```

When Governor is present, `pavctl apply` becomes a **release request** subject to approval.
Governor can approve, defer, or reject, and only approved plans are executed by `pavis-relay`.
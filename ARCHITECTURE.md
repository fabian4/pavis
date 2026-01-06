# Architecture

## 1. Overview

Pavis replaces the traditional "Smart Proxy" model (Envoy) with a **Split Data Plane** architecture. Heavy lifting like parsing, defaulting, and semantic validation is offloaded to the ingest + codec pipeline, keeping the sidecar proxy lightweight and binary-focused while the relay focuses on artifact distribution.

### 1.1 End-to-End Configuration Flow

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
┌──────────────┐      ┌──────────────────┐      ┌───────────────────┐  │   │       pavctl       │      └────────▲─────────┘
│ Static Files │─────▶│ pavis-ingest-file │─────▶│   pavis-codec-*   │──┘   │   (Tool / CLI)     │──────(debug)──┘
└──────────────┘      └───────────────────┘      └───────────────────┘      └────────────────────┘
```

All arrows represent data or artifact flow, not call graphs or control flow.
Fixed dataflow: **Ingest → SourceArtifact → Codec → RuntimeConfig → Relay → PVS Artifact → Runtime**.
Type-level validation flow: **SourceArtifact → CheckedArtifact → RuntimeConfig → ValidatedRuntimeConfig → Relay**.

### 1.2 Sidecar Scope (Outbound-first)

Pavis is **outbound-first**: it primarily targets service-to-service proxying (Linkerd-style).
Inbound use is possible but **optional/future** and tends to pull the runtime toward gateway/policy-engine concerns.
Inbound behavior must be treated as an explicit tradeoff and **must not** leak gateway-style policy logic into the runtime by default.

### 1.3 Terminology (Artifacts and Boundaries)

- **SourceArtifact**: raw source bytes + metadata emitted by ingest (this is `Artifact` in `pavis-ingest-api`).
- **PVS Artifact**: the `.pvs` binary produced from a validated `RuntimeConfig` by `pavis-pvs`.
- **Envelope** (deprecated): avoid this term; use **SourceArtifact** instead.

## 2. Components & Boundaries

| Component              | Description                                                                                       |
| :--------------------- | :------------------------------------------------------------------------------------------------ |
| **`pavis`**            | Proxy – Runtime Engine. Reads optimized `.pvs` binary files only. Executes fully prepared configs.|
| **`pavis-core`**       | Protocol – Canonical types, semantic validation, and memory layout.                               |
| **`pavis-relay`**      | Relay – Versions `.pvs`, manages caches. **Pass-through** for validation; distributes artifacts.  |
| **`pavis-ingest-*`**   | Ingest – Source connectivity (xDS, K8s, file watch): streams, auth, retries, resync.              |
| **`pavis-ingest-api`** | Ingest API – SourceArtifact (raw bytes + metadata) and ingest trait boundary.                     |
| **`pavis-codec-*`**    | Codec – SourceArtifact → RuntimeConfig transforms, **default population**, and core validation via codec-api. |
| **`pavis-codec-api`**  | Codec API – Codec trait boundary for SourceArtifact ↔ RuntimeConfig transforms.                   |
| **`pavis-pvs`**        | Binary Protocol – Integrity layer (Header + Checksum + Encoding).                                 |
| **`pavctl`**           | CLI – Developer tool for manual generation, conversion, and runtime management.                   |

### 2.1 Dependency Graph

*   **`pavis-core` (Root)**: The foundation. Canonical types and semantic validation. No I/O.
*   **`pavis-pvs`**: Depends on `pavis-core`. Handles the binary lifecycle.
*   **`pavis-codec-api`**: Defines the Codec boundary. Depends on `pavis-core` and ingest SourceArtifact types.
*   **`pavis-ingest-api`**: Defines the Ingest boundary (SourceArtifact + metadata).
*   **`pavis-codec-*`**: Pure logic crates. Depend on `pavis-core` and `pavis-codec-api` for the codec boundary and canonical validation in `materialize`.
*   **`pavis-ingest-*`**: Connectivity crates. Handle I/O and transport to upstream sources; emit SourceArtifacts (bytes + metadata) only.
*   **`pavis-relay`**: Coordinates ingest/codec outputs, versions artifacts, and distributes `.pvs`. It does artifact-level header/payload handling only and does not decode DTOs or `RuntimeConfig`.
*   **`pavctl`**: Depends on codecs and `pavis-pvs` to provide manual control and local tooling.
*   **`pavis` (Runtime)**: Depends on `pavis-core` and `pavis-pvs` only. **Must not** depend on ingest/codec/relay/governor.

### 2.2 Responsibilities

| Responsibility     | Component        | Description                                                                                        |
| :----------------- | :--------------- | :------------------------------------------------------------------------------------------------- |
| **Ingest**         | `pavis-ingest-*` | Subscribes to configuration sources. Handles auth, watch/stream, retries, and resync.              |
| **Codec**          | `pavis-codec-*`  | Compiles SourceArtifacts into `RuntimeConfig`, **populates defaults**, and relies on codec-api for canonical validation. |
| **Relay**          | `pavis-relay`    | Versions artifacts and distributes `.pvs`. **Pass-through** for logic; no validation or population.|
| **Governor**       | `pavis-governor` | Admission, policy enforcement, and approval of change plans (future/optional).                     |
| **Manual Tooling** | `pavctl`         | Reuses codecs for local file generation (`gen`), conversion (`convert`), and manual `apply`.       |
| **Integrity**      | `pavis-pvs`      | Computes checksums and adds protocol headers to encoded payloads.                                  |
| **Execution**      | `pavis`          | Zero-copy execution of the binary config. **No validation or default population**.                 |

## 3. Configuration Architecture & Invariants

This section defines the Pavis configuration architecture, strictly separating **Policy** (Codec) from **Mechanism** (Runtime).

### 3.1 Pipeline Stages
Pavis configuration MUST pass through the following stages in order:

1.  **SourceArtifact → CheckedArtifact**: Implemented by codec `check`. Validates framing and basic integrity only.
2.  **CheckedArtifact → RuntimeConfig**: Implemented by codec `compile`. Parses source bytes, normalizes quirks, performs structural completion, and applies **source-specific semantic defaults**.
3.  **RuntimeConfig → Core Semantic Validation**: Implemented by codec-api `materialize` via `pavis-core`. Runtime and Relay MUST NOT compensate for missing intent.

### 3.2 Codec-Internal DTOs (Optional)

Codecs MAY use internal DTOs for parsing and shape normalization, but these are **codec-private** and not enforced or provided by codec-api:

*   **Source DTO**: Represents the source format directly. MAY be sparse. MUST NOT include runtime-specific assumptions.
*   **Structurally Complete DTO**: Shape-complete (no missing containers). MUST NOT introduce semantic defaults beyond structural completion.
*   **RuntimeConfig**: Fully materialized. Semantically final and immutable.

### 3.3 Architectural Invariants

The following rules are absolute.

#### Codec Layer (`pavis-codec-*`)
*   **Role:** Owner of **Defaults** and **Policy**.
*   **Responsibility:** MUST materialize all optional fields into explicit values.
*   **Responsibility:** MUST normalize user input (YAML/JSON/xDS) into `RuntimeConfig` and rely on `Codec::materialize` for canonical validation.
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

### 3.4 Zero-Option Runtime

The Runtime consumes a configuration where all policy decisions are **explicit** and **materialized**. `Option<T>` fields are forbidden for policy configuration.

*   **Explicit State**: Optional features must be represented by explicit enums (e.g., `TlsMode::Disabled` vs `TlsMode::Enabled`) or empty collections.
*   **No Inference**: The Runtime MUST NOT infer defaults from missing values.
*   **Time Semantics**: Durations are materialized as `u32` milliseconds. `0` has specific, normative semantics per field.

## 4. Implementation Internals

### 4.1 Runtime Engine Internals

Pavis operates as an L7 proxy capable of decrypting, inspecting, and re-encrypting traffic. The request lifecycle is as follows:

1.  **Accept**: The listener accepts a raw TCP connection.
2.  **Handshake (Optional)**: If `TlsConfig` is enabled, the runtime performs the server-side TLS handshake using OpenSSL/BoringSSL (via Pingora). Certificates are loaded from disk paths specified in the config.
3.  **Protocol Decode**: The stream is parsed as HTTP/1.1 or H2.
4.  **L7 Match**: The `Router` inspects headers (`Host`, `Path`) against the configuration to select a VirtualHost and Route.
5.  **Action**:
    *   **Proxy**: The request is load-balanced to an upstream.
    *   **Direct**: (If configured) A synthetic response or redirect is generated immediately.

### 4.2 Runtime Memory Lifecycle (RCU)

Pavis achieves lock-free hot reloading using a Read-Copy-Update (RCU) pattern via `arc-swap`.

1.  **Stage**: The **Pavis Runtime** downloads the new `.pvs` file to a temporary location.
2.  **Verify**: Validate Magic Bytes, Checksum, and perform `rkyv::check_archived_root`.
3.  **Map**: Call `mmap` on the valid file.
4.  **Swap**: Atomic pointer swap of the config guard.
5.  **Reclaim**: The old guard is dropped. When the last request RefCount hits 0, `munmap` is invoked.

### 4.3 Networking & Discovery

Pavis supports three distinct discovery modes for upstream clusters, balancing performance with flexibility:

1.  **Static**: Fixed IP addresses and ports. Zero runtime overhead. Used for stable infrastructure or when an external control plane (like the Pavis Relay) performs the resolution and pushes updated configs.
2.  **StrictDns**: The proxy resolves the hostname via DNS and uses the returned A records. It honors TTLs and updates the pool accordingly. Ideal for Kubernetes Headless Services.
3.  **LogicalDns**: The proxy resolves the hostname lazily. Connections are made to the resolved IP, but the pool is not strictly synchronized with all A records. Useful for AWS ALBs or services where the DNS name resolves to a rotating set of functional IPs.

### 4.4 Routing Algorithm (Hot Path)

Routing is hierarchical to minimize CPU cycles.

1.  **Exact Match Table**: O(1) lookup for `(Host, Path)`.
2.  **Prefix Tree**: O(log N) radix tree.
3.  **Regex Pattern List**: Ordered list of regex patterns.
    *   Regex compilation occurs **once** during the "Swap" phase.
    *   Compiled regex state lives in runtime-only wrappers, not the `.pvs` file.

### 4.5 xDS Codec Architecture

The xDS Codec uses an **Intermediate Type Pattern**:
1.  **Decode**: Unmarshal Protobuf bytes into generated Envoy v3 structs.
2.  **Normalize**: Map scattered xDS resources into a coherent internal representation.
3.  **Map**: Transform Envoy structs to `pavis-core` structs.
4.  **Validate**: Performed by the codec pipeline in `Codec::materialize`.

**Responsibility Matrix:**
| Component | Responsibility | Boundary |
| :--- | :--- | :--- |
| **Ingest** | Network I/O with xDS server. | Passes raw xDS Protobuf -> Codec. |
| **Codec** | Pure translation. | Maps Proto enums -> Core enums. 1:1 mapping. No DNS resolution. |
| **Core** | Domain definitions. | Defines *what* a DNS upstream is (the type), but NOT *how* it resolves or refreshes. |
| **Runtime** | Execution & IO. | Handles DNS refresh intervals, TTL, and address family selection. |
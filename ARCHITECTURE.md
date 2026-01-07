# Architecture: Frozen Data Plane

## 1. Overview

Pavis implements the **Frozen Data Plane** architecture. This model rejects the traditional "Smart Proxy" approach where the data plane performs complex parsing, validation, and policy inference at runtime.

Instead, Pavis treats configuration as a compilation target. All routing logic, security policies, and defaults are resolved **Ahead-of-Time (AOT)** by a Codec pipeline. The runtime executes a binary artifact (`.pvs`) that is guaranteed to be valid, complete, and immutable.

### 1.1 Compile-Time Resolution Pipeline

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

The data flow is unidirectional and strictly typed:
**Ingest → SourceArtifact → Codec → RuntimeConfig → Relay → PVS Artifact → Runtime**.

This pipeline enforces the Frozen model:
*   **Ingest**: Raw I/O. No interpretation.
*   **Codec**: Compilation. Resolves defaults, validates semantics, and freezes policy.
*   **Runtime**: Execution. Pure mechanism; no policy capability.

### 1.2 Outbound-First Scope

The Frozen Data Plane model aligns naturally with **sidecar (outbound)** deployments where the configuration scope is bounded (local application dependencies).
Inbound (Gateway) deployments often require dynamic policy evaluation (OIDC, Rate Limiting via Redis) which violates the strict determinism of the Frozen model. While Pavis supports inbound traffic, it refuses to embed dynamic policy engines to support complex gateway use-cases.

### 1.3 Terminology

- **SourceArtifact**: Raw input bytes (YAML, xDS Protobuf) + metadata.
- **PVS Artifact**: The frozen, zero-copy binary artifact executed by the runtime.
- **RuntimeConfig**: The fully materialized, in-memory representation of the frozen state.

## 2. Components & Boundaries

| Component              | Description                                                                                       |
| :--------------------- | :------------------------------------------------------------------------------------------------ |
| **`pavis`**            | **Runtime Engine**. Executes frozen `.pvs` artifacts. **Cannot** validate or infer policy.      |
| **`pavis-core`**       | **Protocol**. Defines the frozen schema and memory layout.                                        |
| **`pavis-relay`**      | **Distribution**. Distributes frozen artifacts. **Pass-through** only; no logic modification.     |
| **`pavis-codec-*`**    | **Compilers**. Transform source intent into frozen `RuntimeConfig`. Owns all policy decisions.    |
| **`pavis-ingest-*`**   | **Ingest**. Handles connectivity to dynamic sources (xDS, K8s API).                               |
| **`pavis-pvs`**        | **Integrity**. Ensures the artifact has not been mutated since compilation.                       |

### 2.1 Responsibilities

The architecture enforces strict separation of concerns to maintain the "Frozen" invariant.

| Responsibility     | Component        | Description                                                                                        |
| :----------------- | :--------------- | :------------------------------------------------------------------------------------------------- |
| **Compilation**    | `pavis-codec-*`  | Compiles vague source intent into explicit, deterministic configuration. Populates all defaults.   |
| **Distribution**   | `pavis-relay`    | Versions and distributes artifacts. **Must not** validate or modify the payload.                   |
| **Execution**      | `pavis`          | Zero-copy execution. **Rejected by Design**: Logic for defaults, inference, or partial validity.   |

## 3. Configuration Architecture: Frozen State

This section defines how the Frozen Data Plane model is enforced through the configuration pipeline.

### 3.1 Pipeline Stages
Configuration **MUST** proceed through these stages. The Runtime cannot accept input from earlier stages.

1.  **SourceArtifact → CheckedArtifact** (Codec): Verify framing.
2.  **CheckedArtifact → RuntimeConfig** (Codec): **Compilation Phase**. Parse, normalize, and populate semantic defaults. The output is a complete, explicit policy.
3.  **RuntimeConfig → ValidatedRuntimeConfig** (Codec): **Validation Phase**. Enforce invariants (e.g., regex safety) before the artifact is sealed.

### 3.2 Architectural Invariants

The following rules are absolute consequences of the Frozen Data Plane.

#### Codec Layer (`pavis-codec-*`)
*   **Role:** Compiler.
*   **Responsibility:** **Materialize Policy**. All "magic" (implicit behaviors) must be converted to explicit configuration instructions.
*   **Output:** A deterministic `RuntimeConfig`.

#### Runtime Layer (`pavis`)
*   **Role:** Executor.
*   **Input:** STRICTLY `ValidatedRuntimeConfig` (via `.pvs`).
*   **Invariant:** **Frozen Logic**. The runtime has no logic to apply semantic defaults (e.g., "missing timeout means 5s"). `None` means "Disabled", never "Default".
*   **Invariant:** **Immutable State**. Configuration cannot be mutated after load.
*   **Failure:** The runtime fails immediately if the artifact is structurally invalid; it does not attempt recovery or compensation.

#### .pvs Artifacts
*   **Nature:** Frozen.
*   **Guarantee:** A `.pvs` compiled today will execute identically on future runtime versions. Policy evolution happens in the Codec, not the Runtime.

### 3.4 Frozen Runtime State

The Runtime consumes a configuration where all policy decisions are **frozen**.

*   **Explicit State**: Features are `Enabled` or `Disabled`. There is no `Auto` state at runtime (except for system resources like threads).
*   **No Inference**: The Runtime executes instructions; it does not interpret intent.
*   **Deterministic Time**: Timeouts are materialized as fixed `u32` milliseconds.

## 4. Implementation Internals

### 4.1 Runtime Engine Internals

Pavis operates as an L7 proxy. Its pipeline is fixed and optimized for the frozen model:

1.  **Accept**: TCP accept.
2.  **Handshake**: TLS handshake using explicit certificates from the frozen config.
3.  **Decode**: HTTP/1.1 or H2 parsing.
4.  **Match**: `Router` executes O(1) or O(log N) lookups against the frozen routing table.
5.  **Action**: Executes the pre-compiled `RouteAction`.

### 4.2 Runtime Memory Lifecycle (RCU)

Hot reloading is achieved via atomic pointer swaps of the frozen state.

1.  **Stage**: Download `.pvs`.
2.  **Verify**: Cryptographic verification of the frozen artifact.
3.  **Map**: Memory-map the artifact.
4.  **Swap**: Atomic replacement of the configuration pointer.
5.  **Reclaim**: Old state is dropped when refcount hits zero.

### 4.3 Networking & Discovery

Discovery is the *only* mutable aspect of the runtime, strictly bounded to endpoint selection.

1.  **Static**: Fixed IPs compiled into the artifact.
2.  **StrictDns / LogicalDns**: The runtime updates endpoint lists based on TTL. This is **mechanism**, not policy. The *decision* to use DNS and the TTL parameters are frozen in the config.

### 4.4 Routing Algorithm (Hot Path)

Routing uses static, optimized structures built during the artifact compilation phase (or mapped directly).
*   Regexes are compiled once during the "Swap" phase.
*   No runtime script evaluation (Lua/WASM) occurs during routing.

### 4.5 xDS Codec Architecture

The xDS Codec functions as a **Compiler**:
1.  **Decode**: Unmarshal Envoy Protobuf.
2.  **Normalize**: Flatten disparate xDS resources into a coherent model.
3.  **Map**: Transform Envoy semantics into Pavis frozen semantics.
4.  **Validate**: Final pass before artifact generation.

The Runtime **never** connects to xDS directly; this would violate the Frozen Data Plane model by introducing runtime complexity and non-determinism.

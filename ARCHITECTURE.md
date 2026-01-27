# Architecture: Frozen Data Plane

**Scope:** This document defines the normative architectural invariants of the Pavis system.
**Status:** Living Constitution. Changes here require broad consensus.

## 1. Frozen Data Plane Axioms

The following axioms are the foundational constraints of the Pavis architecture. All components **MUST** adhere to these rules.

**A1: No Runtime Inference**
The Runtime **MUST NOT** infer policy from missing configuration. "Implicit defaults" are forbidden in the data plane. All defaults, fallbacks, and expansions **MUST** be materialized explicitly by the Codec before execution.

**A2: Immutable Execution State**
The Runtime configuration is immutable. State changes **MUST** occur only via the atomic replacement of the entire PVS artifact. Partial updates or runtime mutation of the active configuration structure are forbidden.

**A3: Layered Validation**
The Codec layer is the sole authority for **format and default resolution**. Canonical semantic validation lives in **Core** (`pavis-core::validate_runtime`). The Runtime **MUST** execute the instructions provided by Core without semantic re-interpretation, but **MUST** enforce environment checks (file readability, port availability) before applying a config.

**A4: Atomic Validity**
A PVS artifact **MUST** be valid in its entirety. If any component of the configuration is malformed or violates a constraint, the Runtime **MUST** reject the entire artifact. "Best-effort" loading of partial configuration is forbidden.

**A5: Relay Opacity**
The Relay **MUST** treat PVS artifacts as opaque binary blobs. It **MUST NOT** modify, validate, inspect, or re-encode the artifact logic. Its responsibility is strictly limited to versioning, persistence, and distribution.

## 2. System Model

Pavis adheres to a strict separation of concerns between compilation, distribution, and execution.

| Component | Responsibility | Constraints |
| :--- | :--- | :--- |
| **Codec** (`pavis-codec-*`) | **Compilation**. Transforms source intent (YAML, xDS) into explicit `RuntimeConfig`. | Must resolve all defaults. Must fail on ambiguity. |
| **Relay** (`pavis-relay`) | **Distribution**. Versions and persists artifacts. | Must be content-agnostic. Must enforce monotonic versioning. |
| **Runtime** (`pavis`) | **Execution**. Forwards traffic based on frozen state. | Efficient execution. No policy inference. |
| **Core** (`pavis-core`) | **Semantics**. Validates canonical invariants for `RuntimeConfig`. | Must reject invalid semantics. |
| **Protocol** (`pavis-core`) | **Schema**. Defines the wire format and memory layout. | Must be versioned. Shared ownership. |

### 2.1 Data Flow

The data flow is unidirectional:
`Source Intent → [Codec] → RuntimeConfig → [Core Validation] → ValidatedRuntimeConfig → [Relay] → PVS Artifact → [Runtime]`

1.  **Ingest**: Raw I/O from sources (File, xDS).
2.  **Compile**: Codec normalizes intent into a fully explicit `RuntimeConfig`.
3.  **Freeze**: `RuntimeConfig` is serialized into a sealed PVS artifact.
4.  **Distribute**: Relay assigns a version and persists the artifact.
5.  **Execute**: Runtime loads the artifact and swaps the execution pointer.

## 3. Configuration Architecture

### 3.1 Pipeline Stages

Configuration processing **MUST** proceed through distinct stages to enforce the Frozen model:

1.  **SourceArtifact**: Raw input bytes (e.g., `pavis.yaml`, xDS Protobuf).
2.  **RuntimeConfig**: The fully materialized, in-memory representation. All policies are resolved.
3.  **ValidatedRuntimeConfig**: Semantic invariants (e.g., regex safety, routing tree integrity) are verified in Core.
4.  **PVS Artifact**: The sealed, binary representation ready for execution.
5.  **Runtime Env Validation**: File and port checks performed just-in-time before apply.

### 3.2 Runtime Invariants

*   **Explicit State**: Configuration fields are strictly typed. `Option<T>` in the Runtime implies "Enabled/Disabled", never "Use Default".
*   **Deterministic Time**: All time-based parameters **MUST** be materialized as fixed integer milliseconds.
*   **Fail-Closed**: If the Runtime encounters a configuration state it cannot execute (e.g., invalid regex that passed validation), it **MUST** abort or fail-closed. It **MUST NOT** fall back to an insecure open state.
*   **TLS SNI Stability**: The Codec may materialize `canonical_sni` to stabilize pooling. `reuse_across_sni` is an explicit, opt-in policy that requires verification to remain enabled.

## 4. Runtime Architecture

### 4.1 Execution Engine

The Runtime operates as a "dumb pipe" optimized for the frozen model.

1.  **Artifact Loading**: The Runtime loads the PVS artifact into memory.
2.  **Atomic State Switching**: Hot reloading is implemented as an atomic pointer swap. The old configuration is dropped only after all active requests referencing it have completed.
3.  **Routing**: Routing decisions are optimized static lookups built at compile time.

### 4.2 Networking & Discovery

*   **Static Endpoints**: Fixed IPs compiled into the artifact.
*   **DNS Resolution**: The Runtime respects the *mechanism* (StrictDNS/LogicalDNS) and *TTL* defined in the frozen config. It **MUST NOT** alter the discovery policy at runtime.
*   **TLS Backend**: The runtime is OpenSSL-only. There is no Rustls fallback; all TLS semantics (inbound mTLS, per-upstream CA, client cert chains) are implemented against the OpenSSL backend.

### 4.3 Resilience Policies

Resilience behaviors are fully materialized in the `RuntimeConfig` and enforced at runtime.

*   **Outlier Detection**: Passive ejection based on consecutive failures. State is ephemeral and reset on reload.
*   **Circuit Breaking**: Request-scoped limits (in-flight, pending). Rejection is immediate (HTTP 503) when limits are exceeded.
*   **Health Checks**: Active probes respect the frozen TLS policy of the upstream.

## 5. Relay Protocol

The Relay serves as the authoritative source of truth for configuration versions.

### 5.1 Versioning Invariants

*   **Relay Authority**: The Relay owns version generation. Clients **MUST NOT** propose versions.
*   **Monotonicity**: Versions **MUST** strictly increment. `new_version = current_version + 1`.
*   **Sentinel**: Version 0 is reserved for the bootstrap "no configuration" state.

### 5.2 Persistence Invariants

*   **LKG Authority**: The "Last Known Good" (LKG) metadata is the single source of truth for the current cluster version.
*   **Atomic Promotion**: A new version is considered "published" **if and only if** it is atomically promoted to LKG.
*   **Crash Safety**: The Relay **MUST** be able to recover its state from the LKG marker after a crash.

### 5.3 Client Interaction

*   **Checksum Validation**: Clients **MUST** verify the integrity of downloaded artifacts using the SHA256 checksum provided by the Relay.
*   **Change Detection**: Clients **SHOULD** use artifact checksums (not just version numbers) to detect changes, avoiding race conditions during long-polling.

### 5.4 Runtime Polling Contract

*   **Canonical ETag Format**: All Relay responses **MUST** use strong ETags formatted as `sha256:<lowercase-hex>`. Weak tags (`W/…`) are explicitly rejected by the runtime.
*   **Latest-Only Fetch**: The Config Agent only downloads the artifact referenced by the Relay's latest ETag. Intermediate versions are never replayed; the Relay is responsible for monotonic integrity.
*   **Applied vs. Rejected Cache**: The runtime tracks both the last applied ETag and the last rejected ETag. Conditional requests (`If-None-Match`) prefer the rejected value to prevent repeated 200 responses for known-bad artifacts.
*   **Rejection Handling**: A validation failure records the offending ETag as "rejected" and keeps serving the Last Known Good artifact. Subsequent polls **MUST** receive 304/204 from the Relay while that ETag is current. Returning 200 for a rejected ETag is logged as a Relay contract violation and ignored.
*   **Backoff Semantics**: Long-poll mode (`wait_ms > 0`) is used only after at least one ETag is known. `Rejected` outcomes trigger a fixed short sleep without contaminating the network backoff counter; network failures continue to honor exponential backoff.

## 6. Operational Contracts

### 6.1 Signal Handling

The Runtime **MUST** respond to standard POSIX signals as follows:

*   **SIGTERM / SIGINT**: Initiate **Graceful Shutdown**.
    *   Stop accepting new connections.
    *   Wait for in-flight requests to complete (up to the configured `drain_timeout`).
    *   Force-close remaining connections after timeout.
*   **SIGKILL**: Immediate termination (OS enforced).

### 6.2 Admin Interface

The Runtime provides a read-only Admin API.

*   **Scope**: Health checks and runtime statistics (e.g., uptime, object counts).
*   **Security**: The Admin API is unauthenticated. It **SHOULD** be bound to a loopback address or protected by network policy.
*   **Constraint**: The Admin API **MUST NOT** expose sensitive configuration secrets (e.g., private keys, raw config bytes).

## Appendix: Non-Normative Implementation Notes

### A.1 xDS Codec
The xDS Codec treats Envoy configuration as an *input language*. It does not aim for strict behavioral parity. Concepts in xDS that violate the Frozen Data Plane (e.g., dynamic scripting filters) are rejected or mapped to deterministic equivalents.

### A.2 Benchmark Context
Benchmark tooling relies on side-channel files (`context.env`) to pass metadata. These are not part of the runtime architecture but are essential for the testing contract.

### A.3 Runtime Mechanism
Current implementations utilize memory-mapped files and `ArcSwap` for RCU-style state management to achieve the atomic state switching requirement.

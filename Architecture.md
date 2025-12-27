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
│   ├── pavis/          # Proxy – Pingora-based traffic engine
│   ├── pavis-core/     # Protocol – Shared rkyv structs & validation
│   ├── pavis-cli/      # CLI – YAML → .pvs compiler & inspector
│   └── pavis-xds/      # Bridge – xDS translator & HTTP config server
└── Cargo.toml          # Workspace configuration
```

### 2.2. Dependency Graph

*   **`pavis-core` (Root)**: The foundation. Depends on **nothing** in the workspace.
*   **`pavis` (Runtime)**: Depends on `pavis-core`. **Must not** depend on `pavis-cli` or `pavis-xds`.
*   **`pavis-cli` / `pavis-xds` (Producers)**: Depend on `pavis-core`.

### 2.3. Responsibilities

| Responsibility | Owner | Description |
| :--- | :--- | :--- |
| **Protocol Definition** | `pavis-core` | Defines `.pvs` format, magic bytes, versioning, and `rkyv` structs. |
| **Semantic Validation** | `pavis-core` | Defines rules for valid data (e.g., "weights > 0"). Shared logic. |
| **Input Validation** | Producers | `cli` & `xds` validate source constraints (YAML, xDS) before conversion. |
| **Loading & Integrity** | `pavis` | Handles `mmap` and `rkyv::check_bytes`. No semantic re-validation. |
| **Runtime Safety** | `pavis` | Handles crash-loop protection and safe execution. |

### 2.4. Layering Principles

1.  **Protocol Definition Layer (`pavis-core`)**
    *   Defines only the protocol and semantics.
    *   Includes `ProxyConfig`, `ArchivedProxyConfig`, and canonical validation.
    *   No dependency on YAML, CLI, or runtime.
    *   Does not perform format conversion or legacy compatibility.

2.  **Boundary Layer (`pvs` crate)**
    *   Handles loading and unpacking.
    *   Responsible for reading `.pvs` files, unwrapping `rkyv::With<T, A>`, and zero-copy access.
    *   Performs version and magic byte checks.
    *   Exposes a safe API to the runtime without leaking archive internals.

3.  **Runtime Layer (`pavis`)**
    *   Consumes only validated configuration.
    *   Does not know about `rkyv`, `mmap`, or archive details.
    *   Does not manipulate binary or protocol internals.
    *   Builds runtime state and performs defensive checks only.

> **Key Idea:** Protocol definitions remain pure, loading is isolated, and the runtime always works with a clean, validated configuration.

## 3. The PVS Protocol

The core innovation of Pavis is the **PVS Protocol**, a zero-copy binary configuration format. See [PROTOCOL.md](doc/PROTOCOL.md) for detailed specification.

### 3.1. File Format

| Offset | Size | Type | Value | Description |
|--------|------|------|-------|-------------|
| `0x00` | 4 | `[u8; 4]` | `PAVS` | Magic bytes – identifies file type |
| `0x04` | 4 | `u32` | `1` | Version – schema version for compatibility |
| `0x08` | ... | `bytes` | ... | Payload – the `ArchivedProxyConfig` root |

### 3.2. Performance Benefits

1. **Zero Parsing** – Pavis uses `mmap` to map the file directly into virtual memory. No parsing step.
2. **Lazy Loading** – If config contains 10,000 routes (50MB) but the app only calls 2 services, the OS only loads the specific 4KB pages needed. The rest stays on disk.

### 3.3. Distribution (Long Polling)

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

## 4. Proxy Runtime Architecture

The `pavis` crate has been refactored into a **Domain-Driven Architecture** with strict module boundaries and invariants.

### 4.1. Modules

1.  **Config (`config`)**:
    *   **Role**: Pure Data Transfer Objects (DTOs) and semantic validation.
    *   **Invariant**: Configuration must be validated (`ValidatedConfig`) before being used by the runtime.
    *   **Key Types**: `Config`, `ValidatedConfig`, `VirtualHost`.

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
    *   **Canonical Truth.** Defines the absolute semantics of the configuration.
    *   Ensures structural correctness and cross-resource invariants (e.g., reference integrity).
    *   Source-agnostic and mandatory for all config producers.

2.  **Source-Specific Validation (`pavis-cli`, `pavis-xds`)**:
    *   **Input Adaptation.** Enforces constraints tied to the input source.
    *   Validates YAML schemas, CLI flags, or specific Istio/Envoy constraints.
    *   Transforms user intent into valid `pavis-core` structures.

3.  **Defensive Runtime Validation (`pavis`)**:
    *   **Operational Safety.** Performs minimal checks to prevent crashes.
    *   Checks magic bytes, version compatibility, and memory safety (`rkyv::check_bytes`).
    *   Does not define or reinterpret semantics.

> **Principle:** Semantics live in `pavis-core`; producers adapt, the proxy executes safely.

### 5.2. Crash-Loop Protection

- Configuration persisted to disk (`/etc/pavis/config.pvs`)
- If Control Plane is down during Pod restart, Pavis loads last known good config and serves traffic immediately

### 5.3. Strategic Filtering

To prevent "Config Bloat" (a major issue in Envoy), the Bridge (`pavis-xds`) performs aggressive filtering before compiling the `.pvs` file.

- **Network Efficiency** – Only sends routes relevant to the specific Pod (based on Namespace or SidecarScope)
- **Security** – A compromised sidecar only knows the IP addresses of services it is explicitly allowed to talk to

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

## 2. Components

```
pavis/
├── crates/
│   ├── pavis/          # Proxy – Pingora-based traffic engine
│   │   ├── src/
│   │   │   ├── config/     # Domain: User Input (DTOs, Validation)
│   │   │   ├── router/     # Domain: Traffic Steering (Matching)
│   │   │   ├── upstream/   # Domain: Backend State (Clusters, LB)
│   │   │   ├── telemetry/  # Domain: Observability (Logs, Metrics)
│   │   │   └── proxy/      # Domain: Runtime Coordination
│   ├── pavis-core/     # Protocol – Shared rkyv structs & validation
│   ├── pavis-cli/      # CLI – YAML → .pvs compiler & inspector
│   └── pavis-xds/      # Bridge – xDS translator & HTTP config server
└── Cargo.toml          # Workspace configuration
```

| Component | Package | Type | Role |
|-----------|---------|------|------|
| **Proxy** | `pavis` | Binary/Lib | Pingora-based engine. Handles traffic, enforces policies. Now modularized into `config`, `router`, `upstream`, `telemetry`, and `proxy`. |
| **Protocol** | `pavis-core` | Library | Shared interface. Defines `ProxyConfig` structs, `rkyv` serialization, validation rules. |
| **Bridge** | `pavis-xds` | Binary | Controller. Connects to Istiod via xDS, translates Protobuf to `.pvs`, serves config via HTTP. |
| **CLI** | `pavis-cli` | Binary | Developer tool. Compiles YAML to `.pvs` and inspects binary files for debugging. |

## 3. Proxy Internal Architecture

The `pavis` crate has been refactored into a **Domain-Driven Architecture** with strict module boundaries and invariants.

### 3.1. Modules

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

### 3.2. Data Flow

```
Request -> Proxy -> Router (Match) -> Upstream Manager (Select Endpoint) -> Cluster -> Endpoint
             │
             ▼
        Telemetry (Log)
```

## 4. Protocol (`.pvs`)

The core innovation of Pavis is the **PVS Protocol**, a zero-copy binary configuration format.

### File Format

| Offset | Size | Type | Value | Description |
|--------|------|------|-------|-------------|
| `0x00` | 4 | `[u8; 4]` | `PAVS` | Magic bytes – identifies file type |
| `0x04` | 4 | `u32` | `1` | Version – schema version for compatibility |
| `0x08` | ... | `bytes` | ... | Payload – the `ArchivedProxyConfig` root |

### Why This Is Faster Than Envoy

1. **Zero Parsing** – Pavis uses `mmap` to map the file directly into virtual memory. No parsing step.
2. **Lazy Loading** – If config contains 10,000 routes (50MB) but the app only calls 2 services, the OS only loads the specific 4KB pages needed. The rest stays on disk.

## 5. Communication (Long Polling)

Pavis avoids the complexity of gRPC bidirectional streams in the sidecar. It uses HTTP Long Polling.

### Flow

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

## 6. Resilience & Safety

### Crash-Loop Protection

- Configuration persisted to disk (`/etc/pavis/config.pvs`)
- If Control Plane is down during Pod restart, Pavis loads last known good config and serves traffic immediately

### Memory Safety

- **Rust** prevents buffer overflows and use-after-free errors common in C++ proxies
- **Validation** – `rkyv` performs `check_bytes` on the memory map to ensure file integrity before use

## 7. Strategic Filtering

To prevent "Config Bloat" (a major issue in Envoy), the Bridge (`pavis-xds`) performs aggressive filtering before compiling the `.pvs` file.

- **Network Efficiency** – Only sends routes relevant to the specific Pod (based on Namespace or SidecarScope)
- **Security** – A compromised sidecar only knows the IP addresses of services it is explicitly allowed to talk to


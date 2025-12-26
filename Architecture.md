# Architecture Design Document

## 1. High-Level Overview

Pavis replaces the traditional "Smart Proxy" model (Envoy) with a **"Split Data Plane"** architecture. Instead of every sidecar performing expensive parsing, Pavis offloads complexity to a centralized bridge, keeping the sidecar lightweight and fast.

### System Diagram

```mermaid

```

## 2. Component Breakdown

```
pavis/
├── crates/
│   ├── pavis/          # The Proxy (Binary) - Pingora-based traffic engine
│   ├── pavis-core/     # The Protocol (Library) - Shared rkyv structs & validation
│   ├── pavis-cli/      # The CLI Tool (Binary) - YAML → .pvs compiler & inspector
│   └── pavis-xds/      # The Bridge (Binary) - xDS translator & HTTP config server
└── Cargo.toml          # Workspace configuration
```

| Component | Package Name | Type | Role |
| :--- | :--- | :--- | :--- |
| **The Proxy** | `pavis` | Binary | The **Pingora-based engine**. Handles traffic, enforces policies, loads config via `mmap`. Does NOT understand xDS. |
| **The Protocol** | `pavis-core` | Library | The **Shared Interface**. Defines `ProxyConfig` structs, `rkyv` serialization, validation rules. |
| **The Bridge** | `pavis-xds` | Binary | The **Controller**. Connects to Istiod via xDS, translates Protobuf to Rune, serves config via HTTP. |
| **The CLI** | `pavis-cli` | Binary | The **Developer Tool**. Compiles YAML to `.pvs` and inspects binary files for debugging. |

---

## 3. The Protocol (`.pvs`)

The core innovation of Pavis is the **PVS Protocol**, a zero-copy binary configuration format.

### File Format Specification
*   **Extension:** `.pvs`
*   **Serialization:** [rkyv](https://github.com/rkyv/rkyv) (Guaranteed Zero-Copy)
*   **Header Structure:**

| Offset | Size | Type | Value | Description |
| :--- | :--- | :--- | :--- | :--- |
| 0x00 | 4 | `[u8; 4]` | `PAVS` | **Magic Bytes**. Identifies the file type. |
| 0x04 | 4 | `u32` | `1` | **Version**. Schema version for compatibility. |
| 0x08 | ... | `bytes` | ... | **Payload**. The `ArchivedProxyConfig` root. |

### Why this is faster than Envoy
1.  **Zero Parsing:** Pavis does not "read" the file. It uses `mmap` to map the disk file directly into virtual memory.
2.  **Lazy Loading:** If the config contains 10,000 routes (50MB), but the app only calls 2 services, the OS only loads the specific 4KB memory pages needed. The rest stays on disk.

---

## 4. Communication Strategy (Long Polling)

Pavis avoids the complexity of gRPC bidirectional streams in the sidecar. It uses a robust **HTTP Long Polling** mechanism.

### The Flow
1.  **Request:** `pavis-proxy` calls `GET http://raven-service/config`.
    *   Header: `X-Pavis-Version: 105` (Current version).
2.  **Hold:** If the Bridge has version 105, it **holds the connection open** (does not reply) for up to 60 seconds.
3.  **Push:** When Istio pushes a change, the Bridge calculates version 106 and immediately responds to the waiting connection.
4.  **Verify:**
    *   Bridge sends `X-Pavis-Checksum: <xxhash>`.
    *   Proxy downloads bytes, calculates hash, and compares.
    *   If valid, it overwrites `config.pvs` on disk.

---

## 5. Resilience & Safety

### Crash-Loop Protection
*   Configuration is persisted to disk (`/etc/pavis/config.pvs`).
*   If the Control Plane is down during a Pod restart, Pavis loads the last known good config from the disk and starts serving traffic immediately.

### Memory Safety
*   **Rust:** Prevents buffer overflows and use-after-free errors common in C++ proxies.
*   **Validation:** `rkyv` performs `check_bytes` on the memory map to ensure the file was not corrupted by disk errors before pointing pointers to it.

---

## 6. Strategic Filtering

To prevent "Config Bloat" (a major issue in Envoy), the Bridge (**pavis-xds**) performs aggressive filtering before compiling the `.pvs` file.

*   **Network Efficiency:** Only sends routes relevant to the specific Pod (based on Namespace or SidecarScope).
*   **Security:** A compromised sidecar only knows the IP addresses of services it is explicitly allowed to talk to.


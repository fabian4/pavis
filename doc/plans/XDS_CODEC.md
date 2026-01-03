# Implementation Plan: Pavis xDS Readiness (Revised)

This document outlines the technical changes required in `pavis-core` and the `pavis` runtime to enable full compatibility with the `pavis-codec-xds` transformation layer.

## 1. Domain Model Changes (`pavis-core`)

We must evolve the schema to support Envoy-compatible semantics while maintaining strict typing.

### A. Listener Configuration
Transition from an implicit single server configuration to a named listener model to support LDS.

- **Rename** `ServerConfig` struct to `Listener`.
- **Add Field** `Listener::name: String` (required, corresponds to Envoy's listener name).
- **Update** `RuntimeConfig`:
  - **Remove** `server` field (breaking change).
  - **Add** `listeners: Vec<Listener>` (Ordered list).
  - *Constraint*: Each `Listener` represents a **flattened** Envoy listener configuration. Complex Envoy features like multiple filter chains, SNI-based chain selection, or dynamic listener matching are out of scope and MUST result in a codec error.

### B. Upstream Addressing & Discovery
Decouple upstream definitions from IP literals to support DNS, avoiding "stringly typed" addresses.

- **Add Enum** `DiscoveryType`:
  - `Static`: Endpoints are fixed IPs provided in config.
  - `LogicalDns`: Endpoints are hostnames; resolved IPs are used. Fallback on failure. Useful for services with changing IPs but stable DNS.
  - `StrictDns`: Endpoints are hostnames; resolved IPs **replace** the existing set. Respects TTL. Useful for headless services.
- **Add Enum** `EndpointAddress`:
  - `Ip(SocketAddr)`: Strongly typed IP + Port.
  - `Dns(String, u16)`: Hostname + Port.
- **Modify** `Endpoint`: Replace `ip: IpAddr` with `address: EndpointAddress`.

### C. Header Manipulation Semantics
Support Envoy's explicit append vs. overwrite logic for multi-value headers.

- **Add Enum** `HeaderActionType`:
  - `Set`: Overwrite any existing values with this value (Envoy `append: false`).
  - `Append`: Add a new value (Envoy `append: true`).
  - `AddIfAbsent`: Add only if the header key is missing.
  - `Remove`: Remove the header entirely.
- **Modify** `HeaderAction`: Use `action: HeaderActionType` instead of implicit logic.
- *Clarification*: `pavis-core` defines the **intent** (e.g., "Append"). The `pavis` runtime is solely responsible for ensuring RFC-correct emission (e.g., comma-joining standard headers vs. emitting multiple `Set-Cookie` lines).

### D. Route Rewrites
Support a safe subset of rewrite actions. Regex and template rewrites are explicitly **out of scope**.

- **Add Struct** `RewritePolicy`:
  - `path_prefix_rewrite: Option<String>` (Replaces the matched prefix segment of the path).
  - `host_rewrite_literal: Option<String>` (Replaces the `Host` / `:authority` header).
- **Modify** `Route`: Add `rewrite: Option<RewritePolicy>`.

---

## 2. Runtime Logic Changes (`pavis` runtime)

### A. Deterministic Listener Selection
- **CLI Flag**: Add `--listener <name>` to the binary.
- **Boot Logic**:
  1. Load `RuntimeConfig`.
  2. If `listeners` is empty: **Error**.
  3. If `listeners` has exactly 1 entry: Select it automatically (preserves simple use case for non-xDS users).
  4. If `listeners` has >1 entry: Require `--listener <name>`. **Error** if missing or no match found.
  - *Rationale*: Prevents "first one wins" ambiguity while keeping simple deployments ergonomic.

### B. Async DNS Resolver
- **New Component**: `UpstreamResolver` (managed via background tasks/channels).
- **Responsibilities**:
  - Scan `upstreams` on config load/reload.
  - Spawn resolution tasks for `LogicalDns` and `StrictDns` types using `hickory-dns` or `tokio::net`.
  - **LogicalDns Behavior**: Resolve continuously. The LoadBalancer MUST use exactly **one** active endpoint IP at a time (best effort). On failure, **keep existing IP** (LKG).
  - **StrictDns Behavior**: Resolve continuously. The LoadBalancer MUST use the **full set** of resolved IPs. On success, **replace** the set strictly.
- **Integration**: Must update the `LoadBalancer` state atomics (`ArcSwap`) without blocking the proxy request path.

### C. Request Processing Pipeline
- **Header Pipeline**: Update `header_ops.rs` to implement `HeaderActionType`. Ensure `Append` correctly handles HTTP multi-value semantics (e.g., standard headers get comma-appended, special headers like `Set-Cookie` are emitted multiple times).
- **Rewrite Filter**: Implement rewrite logic in `proxy_service.rs` **after** route matching but **before** upstream selection.
  - *Invariant*: Route matching ALWAYS occurs on the **original** request (path + host) before any rewrite is applied.
  - `path_prefix_rewrite`: Identify the portion of the path that matched the route prefix and substitute it.
  - `host_rewrite_literal`: Update `req_header` `Host` and internal context variables used for SNI/TLS.

---

## 3. Responsibility Matrix

| Component | Responsibility | Boundary |
| :--- | :--- | :--- |
| **Ingest** | Network I/O with xDS server. | Passes raw xDS Protobuf -> Codec. |
| **Codec** | Pure translation. | Maps Proto enums -> Core enums. 1:1 mapping. No DNS resolution. |
| **Core** | Domain definitions. | Defines *what* a DNS upstream is (the type), but NOT *how* it resolves or refreshes. |
| **Runtime** | Execution & IO. | Handles DNS refresh intervals, TTL, and address family selection. Holds mutable state. |

---

## 4. Implementation Phasing

### Phase 1: Core Schema Evolution & Migration
*Goal: Unblock `pavis-codec-xds` development.*
1.  **Refactor**: Rename `ServerConfig` to `Listener` and introduce `Vec<Listener>` in `RuntimeConfig`.
2.  **Types**: Add `HeaderActionType`, `RewritePolicy`, and `EndpointAddress` enum to `pavis-core`.
3.  **Migration**: Update `pavctl`, `pavis-codec-serde`, and `pavis-ingest-file` to produce/consume the new schema.
    - *Risk Acknowledgement*: This `server` -> `listeners` migration is schema-breaking. A explicit compatibility strategy (e.g., PVS versioning or a migration tool) is **REQUIRED** to avoid blocking existing non-xDS users.

### Phase 2: Runtime Static Features
*Goal: Feature parity for HTTP traffic manipulation.*
1.  **Listener Selection**: Implement the `--listener` flag and boot logic in `pavis::main`.
2.  **Traffic Logic**: Implement `HeaderActionType` logic (append vs set) and `RewritePolicy` application in the proxy filter.

### Phase 3: Dynamic Upstreams (DNS)
*Goal: Enable dynamic environments (Kubernetes/AWS).*
1.  **Resolver**: Implement `UpstreamResolver` with async DNS.
2.  **State Management**: Implement atomic hot-swapping of upstream endpoints based on resolver results.
3.  **LKG**: Ensure resolution failures do not flap traffic (hold last valid state).

---

## 5. Risks & Trade-offs

1.  **Migration Friction**: Removing `server` field breaks existing PVS files. We will version the PVS format or require a `pavctl convert` step for upgrade.
2.  **DNS Latency**: Resolution is async, but initial resolution might delay startup or first request. The runtime should start "healthy" but fail requests to DNS upstreams until the first resolution completes.
3.  **Rewrite Complexity**: `path_prefix_rewrite` depends heavily on accurate normalization of the request path. We must ensure the "matched prefix" is tracked accurately during routing.

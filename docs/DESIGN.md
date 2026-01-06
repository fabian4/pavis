# Pavis Design Philosophy & Decisions

This document outlines the core design principles and architectural decision records (ADRs) that guide the development of the Pavis project. It serves as a companion to `ARCHITECTURE.md`.

## Design Philosophy

### Feature Constraints

To ensure predictable performance and strict operational characteristics, Pavis deliberately excludes certain capabilities found in general-purpose proxies.

#### 1. No SNI (Single Certificate per Listener)
Pavis is optimized for the **Sidecar Model**, where the proxy represents a single workload identity.
*   **Constraint**: A listener binds to a port and serves exactly one TLS certificate.
*   **Rationale**: Multi-tenant SSL termination adds significant complexity to the configuration model and runtime matching logic. In a mesh environment, identity is tied to the pod/workload, not the hostname.

#### 2. No Inline Certificates
Configuration files (`.pvs` or source YAML) must not contain sensitive key material.
*   **Constraint**: TLS configuration accepts **File Paths** only.
*   **Rationale**:
    *   **Security**: Prevents secrets from leaking into configuration dumps, logs, or the Relay's version history.
    *   **Performance**: Keeps the binary artifact small.
    *   **Operations**: Allows rotating certificates on disk (e.g., via Kubernetes Secrets or cert-manager) without requiring a full configuration reload or process restart. Pingora’s engine can hot-reload these files.

#### 3. No Regex Rewrites
While Regex *matching* is supported for routing, Regex *rewriting* (capture groups and substitution) is explicitly rejected.
*   **Constraint**: Rewrites are limited to `Prefix` replacement and `HostLiteral` replacement.
*   **Rationale**: Regex substitution is computationally expensive and introduces unpredictable latency variance per request. It also opens the door to ReDoS (Regular Expression Denial of Service) attacks if patterns are not carefully crafted. Prefix rewriting covers the vast majority of service mesh use cases (e.g., stripping `/api/v1`).

---

## Architectural Decision Records (ADR)

### ADR-007: TLS & L7 Capabilities

*   **Status**: Accepted
*   **Date**: 2026-01-06
*   **Context**:
    Pavis was originally conceived with a strong focus on high-performance TCP forwarding. However, to function effectively as a Service Mesh sidecar, it must make routing decisions based on HTTP headers, paths, and methods. In a Zero-Trust environment, traffic is encrypted (mTLS). To route based on L7 attributes, the proxy must terminate TLS.

*   **Decision**:
    Pavis **WILL** support inbound Server-side TLS Termination. The Runtime Engine is officially classified as an L7 Proxy, capable of decrypting traffic before the routing phase.

*   **Consequences**:
    *   **Runtime Complexity**: The request processing pipeline must now include a conditional TLS handshake step before protocol parsing.
    *   **Configuration**: The `Listener` struct in `pavis-core` must support TLS configuration fields (cert/key paths).
    *   **Performance**: While OpenSSL/BoringSSL adds overhead, this is necessary trade-off for L7 routing.
    *   **Future Proofing**: This establishes the foundation for future mTLS (mutual TLS) and workload identity features.

### ADR-008: Routing & Actions

*   **Status**: Accepted
*   **Date**: 2026-01-06
*   **Context**:
    Users require the ability to modify traffic behavior beyond simple forwarding. Common requirements include redirecting HTTP to HTTPS, returning immediate errors (e.g., maintenance mode), and rewriting paths for backend compatibility.

*   **Decision**:
    1.  **Direct Actions**: Support `Redirect` (3xx) and `DirectResponse` (synthetic body/status) as first-class route actions.
    2.  **Rewrites**: Support high-performance `Prefix` path rewriting and `Host` header rewriting.

*   **Consequences**:
    *   **Schema**: The `Route` struct must evolve from a simple `destinations` list to a more flexible `Action` enum (or similar structure) that allows *either* forwarding *or* a direct response.
    *   **Validation**: Validation logic must ensure that rewrites are not applied to incompatible match types (e.g., regex matches cannot use prefix rewriting).

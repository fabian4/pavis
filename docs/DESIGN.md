# Pavis Design Philosophy & Decisions

This document outlines the core design principles and architectural decision records (ADRs) that guide the development of the Pavis project. It serves as a companion to `ARCHITECTURE.md`.

## Design Philosophy: Frozen Data Plane

Pavis adheres to the **Frozen Data Plane** architectural model. This dictates that the runtime engine must be a pure, deterministic mechanism for executing a pre-compiled policy.

**Implication**: Any feature that requires unbounded runtime state, dynamic code generation, or per-request policy inference is **out of scope**.

### Feature Constraints

To ensure predictable performance and strict operational characteristics, Pavis deliberately excludes capabilities that violate the Frozen model.

#### 1. No SNI (Single Certificate per Listener)
Pavis is optimized for the **Sidecar Model**, where the proxy represents a single workload identity.
*   **Constraint**: A listener binds to a port and serves exactly one TLS certificate.
*   **Rationale (Frozen Model)**: Multi-tenant SSL termination introduces runtime decision branching dependent on unbounded inputs (SNI headers). By freezing the certificate at listener bind time, we guarantee constant-time handshake logic.

#### 2. No Inline Certificates
Configuration files (`.pvs`) must not contain sensitive key material.
*   **Constraint**: TLS configuration accepts **File Paths** only.
*   **Rationale (Frozen Model)**:
    *   **Immutability**: The configuration structure is immutable. Secrets (keys) have a different lifecycle than policy.
    *   **Security**: Prevents secrets from being frozen into the `.pvs` artifact, which is distributed via the Relay.

#### 3. No Regex Rewrites
While Regex *matching* is supported for routing, Regex *rewriting* (capture groups and substitution) is explicitly rejected.
*   **Constraint**: Rewrites are limited to `Prefix` replacement and `HostLiteral` replacement.
*   **Rationale (Frozen Model)**: Regex substitution introduces unpredictable latency variance and memory allocation per request. The Frozen Data Plane demands bounded execution time; unbounded regex operations violate this guarantee.

---

## Architectural Decision Records (ADR)

### ADR-007: TLS & L7 Capabilities

*   **Status**: Accepted
*   **Date**: 2026-01-06
*   **Context**:
    Pavis requires L7 routing logic to function as a service mesh sidecar. However, L7 inspection requires decryption, which adds complexity to the frozen runtime.

*   **Decision**:
    Pavis **WILL** support inbound Server-side TLS Termination. The Runtime Engine is capable of decrypting traffic before the routing phase.

*   **Consequences**:
    *   **Runtime Complexity**: The request pipeline includes a conditional TLS handshake.
    *   **Frozen State**: To maintain the Frozen model, the certificates used for termination must be explicitly defined in the frozen configuration artifact (via paths), not negotiated dynamically via an external certificate provider at runtime.

### ADR-008: Routing & Actions

*   **Status**: Accepted
*   **Date**: 2026-01-06
*   **Context**:
    Users require the ability to modify traffic behavior beyond simple forwarding.

*   **Decision**:
    1.  **Direct Actions**: Support `Redirect` (3xx) and `DirectResponse` (synthetic body/status).
    2.  **Rewrites**: Support high-performance `Prefix` path rewriting and `Host` header rewriting.

*   **Consequences**:
    *   **Schema**: The `Route` struct evolves to support an `Action` enum.
    *   **Validation (Codec)**: The **Codec** (not the Runtime) acts as the compiler that validates these actions. For example, the Codec ensures that a regex match does not attempt to use a prefix rewrite, enforcing validity before the runtime ever starts.

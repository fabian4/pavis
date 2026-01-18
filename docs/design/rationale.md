# Design Rationale: The Frozen Data Plane

Pavis adopts the **Frozen Data Plane** model to solve the problem of runtime non-determinism in sidecars. The architectural invariants enforcing this are defined in `ARCHITECTURE.md`. This section explains *why* those invariants exist.

## Rationale for Key Constraints

### 1. Single Certificate per Listener (No SNI)
*   **Context**: General-purpose proxies often support SNI (Server Name Indication) to serve multiple domains on a single port.
*   **Decision**: Pavis restricts listeners to a single certificate.
*   **Why**:
    *   **Sidecar Identity**: In a service mesh, a sidecar represents a single workload identity. Multi-tenant termination is an edge gateway concern, not a sidecar concern.
    *   **Determinism**: SNI parsing introduces variable latency and branching logic during the TLS handshake. Eliminating SNI guarantees constant-time handshake logic (O(1)).

### 2. File-Based Certificates (No Inline Secrets)
*   **Context**: Some proxies allow embedding private keys directly in the configuration YAML/JSON.
*   **Decision**: Pavis requires certificates to be referenced by file path.
*   **Why**:
    *   **Lifecycle Separation**: Policy (routing) and Secrets (keys) have different rotation cycles. Freezing secrets into the configuration artifact would force a full config reload just to rotate a cert.
    *   **Security**: The PVS artifact is distributed via the Relay. keeping secrets out of the artifact reduces the blast radius if an artifact is leaked.

### 3. Regex Matching Only (No Rewrites)
*   **Context**: Users often want to rewrite paths using capture groups (e.g., `s/^\/api\/v1\/(.*)/\/$1/`).
*   **Decision**: Pavis supports Regex for *routing* (matching) but rejects Regex for *rewriting* (substitution).
*   **Why**:
    *   **Bounded Execution**: Regex substitution involves memory allocation and variable CPU cost proportional to the input string length and match complexity. This violates the "Bounded Execution" goal of the Frozen Data Plane.
    *   **Alternative**: Prefix and Host literal rewrites cover 90% of sidecar use cases with O(1) cost.

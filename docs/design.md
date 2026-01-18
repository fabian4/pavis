# Design Philosophy: The Frozen Data Plane

Pavis adopts the **Frozen Data Plane** model to solve the problem of runtime non-determinism in sidecars. The architectural invariants enforcing this are defined in `/ARCHITECTURE.md`. This document explains *why* those invariants exist through six key design constraints.

---

## 1. Single Certificate per Listener (No SNI)

**Context:** General-purpose proxies often support SNI (Server Name Indication) to serve multiple domains on a single port.

**Decision:** Pavis restricts listeners to a single certificate.

**Rationale:**
- **Sidecar Identity**: In a service mesh, a sidecar represents a single workload identity. Multi-tenant termination is an edge gateway concern, not a sidecar concern.
- **Determinism**: SNI parsing introduces variable latency and branching logic during the TLS handshake. Eliminating SNI guarantees constant-time handshake logic (O(1)).

---

## 2. File-Based Certificates (No Inline Secrets)

**Context:** Some proxies allow embedding private keys directly in the configuration YAML/JSON.

**Decision:** Pavis requires certificates to be referenced by file path.

**Rationale:**
- **Lifecycle Separation**: Policy (routing) and Secrets (keys) have different rotation cycles. Freezing secrets into the configuration artifact would force a full config reload just to rotate a cert.
- **Security**: The PVS artifact is distributed via the Relay. Keeping secrets out of the artifact reduces the blast radius if an artifact is leaked.

---

## 3. Regex Matching Only (No Rewrites)

**Context:** Users often want to rewrite paths using capture groups (e.g., `s/^\/api\/v1\/(.*)/\/$1/`).

**Decision:** Pavis supports Regex for *routing* (matching) but rejects Regex for *rewriting* (substitution).

**Rationale:**
- **Bounded Execution**: Regex substitution involves memory allocation and variable CPU cost proportional to the input string length and match complexity. This violates the "Bounded Execution" goal of the Frozen Data Plane.
- **Alternative**: Prefix and Host literal rewrites cover 90% of sidecar use cases with O(1) cost.

---

## 4. TLS Termination Strategy (File-Based Keys)

**Context:** L7 inspection requires decryption, but handling keys adds complexity to the frozen runtime. Traditional proxies often embed certificates inline or use complex key management systems.

**Decision:** Support inbound Server-side TLS Termination using file-based keys only.

**Rationale:**
- **L7 Capability**: Enables L7 routing and header manipulation while maintaining the frozen data plane model.
- **Simplicity**: File-based keys keep the runtime simple—it validates file existence at startup before binding listeners.
- **Lifecycle Separation**: Consistent with constraint #2 (File-Based Certificates), ensuring secrets management stays separate from policy distribution.

---

## 5. Action Primitives (No Turing-Complete Scripting)

**Context:** Users need to modify traffic (redirects, rewrites, synthetic responses), but Turing-complete scripting (Lua, WASM) introduces runtime non-determinism.

**Decision:** Implement a fixed set of atomic actions:
- `Redirect` (3xx)
- `DirectResponse` (Synthetic 200/400/503)
- `Rewrite` (Prefix & Host only)

**Rationale:**
- **Compile-Time Validation**: All actions are validated at compile time by the Codec. The runtime executes them as simple data transformations with bounded execution cost.
- **Predictability**: No runtime interpretation, no unbounded loops, no memory allocation variability.
- **Trade-off**: Less flexible than Lua/WASM, but complex logic belongs in upstream services, not the sidecar.

---

## 6. TLS Backend Flexibility (Rustls vs. OpenSSL)

**Context:** Pingora's default `rustls` backend lacks features required for Enterprise mesh environments (inbound mTLS, custom CA verification). However, forcing OpenSSL on everyone increases complexity and binary size.

**Decision:** Support both `rustls` (default) and `OpenSSL` (feature-gated) backends.

**Rationale:**
- **Pragmatic Defaults**: Most users don't need mTLS. `rustls` provides a modern, memory-safe default with smaller binaries.
- **Enterprise Support**: Users requiring mTLS can enable the OpenSSL feature at build time.
- **Validation Integrity**: The Codec validates configuration against target backend capabilities, preventing runtime surprises.

**See Also:** `docs/operations/runtime.md` for specific feature limitations per backend.

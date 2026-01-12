# Phase 4: Security & Identity Implementation Plan

**Status**: Updated (Incorporating E2E Review)
**Reference**: [ROADMAP.md](../../ROADMAP.md) - Phase 4

This document outlines the technical implementation plan for the "Security & Identity" phase, focusing on mTLS, SPIFFE identity extraction, and Route-based Authorization (RBAC).

---

## 0. Baseline: TLS Termination (Verified)

**Status**: ✅ Verified via `tests/suites/pavis/61_security_termination.sh`.
**Action**: Ensure server-side TLS remains stable during Phase 4 implementation. The baseline test covers certificate loading and basic HTTPS handshake.

---

## 1. mTLS (Mutual TLS) - Inbound

**Goal**: Configure Pavis listeners to validate client certificates during the TLS handshake.

### Implementation Tasks

1.  **OpenSSL Configuration in `main.rs`**:
    - Update `crates/pavis/src/main.rs` to configure the `SslAcceptorBuilder`.
    - **Logic**:
        - `ClientAuth::Optional`: Set `SslVerifyMode::PEER`. Load CA file via `set_client_ca_list`.
        - `ClientAuth::Required`: Set `SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT`.
    - **CA Store Requirement**: `set_client_ca_list` only advertises acceptable CAs. You must also set a verify store (e.g., `set_ca_file`/`set_verify_locations` or an explicit `X509Store`) so OpenSSL can validate client certs.
    - **Config Constraint**: If `ClientAuth` is `Optional` or `Required`, a CA bundle / verify store MUST be configured. Missing CA store is a configuration error and must fail fast at startup.
    - **Handshake Behavior**: In `Required` mode, client cert validation failures (unknown CA, invalid chain, expired cert) must fail at TLS handshake time (TLS alert), not as an HTTP 403.
    - **Integration**: Leverage Pingora's `ListenHttp` or `TlsSettings` to apply these `openssl` modes and CA store configuration.

2.  **E2E Test Case**: `tests/suites/pavis/62_security_mtls_handshake.sh`
    - Verify that connection is rejected if no cert is provided (when `Required`).
    - Verify that connection is accepted if valid cert is provided.

---

## 2. Identity Extraction & Workload Identity

**Goal**: Extract the SPIFFE ID (URI SAN) from client certificates and support Pavis's own identity for outbound requests.

### Implementation Tasks

1.  **Inbound Identity Binding**:
    - Implement `connected_to_downstream` in `Proxy` (`crates/pavis/src/proxy/service.rs`).
    - Call `IdentityExtractor::extract` to populate `RouterContext.client_identity`.
    - This allows authorization logic to know "who" the caller is.

2.  **Outbound Workload Identity (mTLS to Upstreams)**:
    - **Status**: Currently `TODO` in `upstream_peer` method of `Proxy`.
    - **Action**: Implement loading of client certificates for `HttpPeer`.
    - **Integration**: Use Pingora APIs to set **client cert + private key** (and optional full chain). If upstream validation is required, also set the upstream CA bundle/verify store.
    - **Config Constraint**: For outbound mTLS, both client cert and private key are required. Missing either is a configuration error and must fail fast at startup.
    - **Chain Handling**: Allow either a PEM with the full chain embedded or a separate optional chain path.
    - **Upstream Verification Default**: Verification is enabled by default using system roots; allow an explicit CA bundle override.
    - **SNI / Server Name Rule**: Use upstream host/endpoint host as the `server_name` for SNI and verification; successful verification depends on this value matching the upstream cert SAN/CN.
    - **Workload Identity**: In a SPIRE environment, these paths (`/run/spire/sockets/...`) point to the workload SVIDs.

---

## 3. Authorization (RBAC) - Static Policies

**Goal**: Enforce "Deny-by-default" RBAC based on the `Principal` requirement of a route.

### Implementation Tasks

1.  **Deny-by-Default Enforcement**:
    - In `request_filter` (`crates/pavis/src/proxy/service.rs`), after a route is matched:
        - If `route.principal` is not `Principal::Any`, and `ctx.client_identity` is `None`, **REJECT** (403).
        - If `route.principal` is `Principal::Authenticated { spiffe }`, and `ctx.client_identity != Some(spiffe)`, **REJECT** (403).
        - If `route.principal` is `Principal::Prefix { prefix }`, and `ctx.client_identity` does not start with `prefix`, **REJECT** (403).
    - **Ordering / Fallback**: Do not rely solely on `connected_to_downstream` ordering. In `request_filter`, if `ctx.client_identity` is `None`, perform fallback extraction from the current connection TLS session (peer cert) and populate/override identity for that request.
    - **Identity Normalization**:
        - Consider only URI SAN entries with scheme `spiffe`.
        - If none, set `client_identity = None`.
        - If exactly one, parse and canonicalize; if invalid, set `client_identity = None`.
        - If more than one, treat as ambiguous and set `client_identity = None` (deny-by-default).
        - Canonicalization: scheme must be lowercase `spiffe`, trust domain + path must be present, path must be non-empty; store as canonical string (or `SpiffeId` newtype canonical string).
    - **RBAC Comparison**: Compare against the canonical representation (strict equality for `Authenticated`, `starts_with` for `Prefix`).

2.  **Telemetry**:
    - Ensure RBAC denials are captured in access logs with a specific error flag or status code (403).

---

## 4. Testing & Verification Matrix

| Test Case ID | Name | Focus | Status |
|--------------|------|-------|--------|
| `60` | `security_tls_origination` | Outbound TLS (Pavis -> Upstream) | ✅ Existing |
| `61` | `security_termination` | Inbound TLS (Client -> Pavis) | ✅ Added |
| `62` | `security_mtls` | Inbound mTLS Handshake & CA Validation | ⏳ Planned |
| `63` | `security_rbac_spiffe` | SPIFFE ID Authorization (Principal match) | ⏳ Planned |
| `64` | `security_rbac_prefix` | Identity Prefix Authorization | ⏳ Planned |

**TLS Mode Prerequisites**:
- `62` runs in `Required` mode at minimum; optionally also run in `Optional` mode.
- `63` and `64` require listener mTLS `Optional` to allow "no identity" while still enabling identity when cert present.

### Scenarios for `63_security_rbac_spiffe.sh`:
1.  **Matched**: Cert with `spiffe://cluster/ns/prod/sa/app1` -> Route requiring `app1` -> **200 OK**.
2.  **Mismatched**: Cert with `spiffe://cluster/ns/prod/sa/app2` -> Route requiring `app1` -> **403 Forbidden**.
3.  **No Identity**: Listener is mTLS `Optional`; omit client cert -> Route requiring `Authenticated` -> **403 Forbidden**.

### Scenarios for `62_security_mtls_handshake.sh` (Acceptance Criteria):
1.  **No Cert (Required)**: Omit client cert -> TLS handshake fails (not HTTP 403).
2.  **Valid Cert**: Provide valid cert signed by configured CA -> TLS handshake succeeds.
3.  **Invalid/Unknown CA**: Provide cert signed by unknown CA -> TLS handshake fails.

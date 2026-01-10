# E2E Test Audit - Phase 1: Coverage of System Responsibilities

- Audit Phase: Phase 1 (Coverage of System Responsibilities)
- Target Module: E2E
- Generation Timestamp: 2026-01-10T05:20:00Z
- AI Model Identifier: Gemini 2.0 Flash

## 1. Responsibility Mapping

The E2E tests are mapped to the primary responsibilities of the Pavis system as follows:

### 1.1 Configuration Ingestion & Artifact Generation
- **Mechanism**: Use of `pavctl gen` (via `gen_pvs` helper in `tests/lib/env.sh`) to transform YAML into `.pvs` binary artifacts.
- **Coverage**: 
    - Exercised in nearly every test case (e.g., `pavis/10_bootstrap_static.sh`).
    - `gen_minimal_pvs` in `tests/lib/env.sh` provides a baseline for minimal valid artifacts.

### 1.2 Distribution & Hot-Reload
- **Control Plane Side**: `pavis-relay` distribution is tested in the `relay` suite.
    - `relay/20_longpoll_wait.sh`: Blocks until an update is available.
    - `relay/30_fanout_multi.sh`: Verifies one publish event wakes up multiple subscribers.
- **Data Plane Side**: `pavis` runtime consumption is tested in the `pavis` suite.
    - `pavis/20_reload_norestart.sh`: Proof of version increment via long-poll without restart.
- **End-to-End**: `integrated/10_bootstrap_path.sh` validates the full chain from compile to active proxy.

### 1.3 Runtime Routing & Forwarding
- **Matcher Logic**: `pavis/40_traffic_matcher.sh` verifies that routing precedence (prefix vs exact) is enforced and can be updated.
- **Load Balancing**: `pavis/41_traffic_weighted.sh` validates that weighted traffic shifts between backends works as specified in the artifact.
- **Security**: `pavis/60_security_tls.sh` verifies that TLS origination to upstreams can be toggled via reload.

### 1.4 Error Handling & LKG
- **Artifact Integrity**: `pavis/30_lkg_corrupt.sh` ensures that if the relay serves a non-PVS file (e.g., random bytes), the runtime maintains the Last-Known-Good state.
- **System Monotonicity**: `relay/11_contract_republish.sh` ensures that the relay rejects configuration rollbacks (duplicate versions).

## 2. Positive vs Negative Coverage

### 2.1 Success-Path (Positive)
The vast majority of tests verify the "happy path" of system evolution:
- Successful bootstrap (`pavis/10_bootstrap_static.sh`).
- Successful route evolution (`pavis/40_traffic_matcher.sh`).
- Successful relay persistence (`relay/50_persistence_recovery.sh`).

### 2.2 Failure-Path (Negative)
The suite includes critical negative scenarios:
- **Corrupt Artifacts**: `pavis/30_lkg_corrupt.sh` publishes raw string data to verify runtime resilience.
- **Monotonicity Violation**: `relay/11_contract_republish.sh` attempts to publish an existing version to trigger a `409 Conflict`.
- **Empty Payloads**: `relay/71_limits_empty.sh` verifies handling of zero-byte publications.
- **Planned Coverage**: `pavis/31_lkg_incompatible.sh` is reserved for semantic rejection (e.g., binding to privileged ports).

## 3. Boundary Validation

- **Artifact Opaqueness**: `relay/10_contract_opaque.sh` explicitly validates that the relay handles random bytes as artifacts, proving it does not interpret or semantically validate the `.pvs` content.
- **Runtime Integrity**: `pavis/10_bootstrap_static.sh` confirms the runtime starts only when provided with a valid `.pvs` file, respecting the boundary that the data plane does not handle raw YAML.
- **Isolation of Concerns**: The separation of `relay` and `pavis` suites ensures that distribution logic (concurrency, persistence) is verified independently of traffic proxying logic.
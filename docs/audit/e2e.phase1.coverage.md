# E2E Test Audit - Phase 1: Coverage of System Responsibilities

- Audit Phase: Phase 1 (Coverage of System Responsibilities)
- Target Module: E2E
- Generation Timestamp: 2026-01-10T06:16:00Z
- AI Model Identifier: Gemini 2.0 Flash

## 1. Responsibility Mapping

The E2E tests cover the following system responsibilities:

### 1.1 Config Ingestion & Artifact Generation
- **`pavis/10_bootstrap_static.sh`**: Uses `pavctl gen` to compile YAML to `.pvs`.
- **`integrated/10_bootstrap_path.sh`**: Validates the end-to-end ingestion flow from compiler to active runtime.

### 1.2 Distribution / Reload
- **`relay/` Suite**: Extensively tests the `pavis-relay` distribution logic, including long-polling (`relay/20_longpoll_wait.sh`) and multi-subscriber fanout (`relay/30_fanout_multi.sh`).
- **`pavis/20_reload_norestart.sh`**: Proves that `pavis` runtime consumes updates via long-poll without process interruption.

### 1.3 Runtime Routing & Forwarding
- **`pavis/40_traffic_matcher.sh`**: Verifies dynamic evolution of path matching logic.
- **`pavis/41_traffic_weighted.sh`**: Verifies weighted load balancing.
- **`pavis/60_security_tls.sh`**: Verifies TLS origination policy changes.

### 1.4 Error Handling Paths
- **`pavis/30_lkg_corrupt.sh`**: Validates fallback to Last-Known-Good configuration when a corrupt artifact is served.
- **`relay/11_contract_republish.sh`**: Validates version monotonicity enforcement at the API boundary.

## 2. Positive vs Negative Coverage

### 2.1 Success-path (Positive)
- Core functionality: bootstrap, routing, reload, persistence are well-covered across all suites.

### 2.2 Failure-path (Negative)
- **Invalid binary artifact**: `pavis/30_lkg_corrupt.sh` (Integrity check).
- **Stale version update**: `relay/11_contract_republish.sh` (Monotonicity check).
- **Oversized payload**: `relay/70_limits_oversize.sh` (Resource limit check).
- **Semantically invalid config**: `pavis/31_lkg_incompatible.sh` (e.g., privileged port bind - status: Planned/Manual).

## 3. Boundary Validation

- **Artifact Consumption**: Tests strictly enforce the boundary that `pavis` runtime only consumes `.pvs` artifacts (`pavis/10_bootstrap_static.sh`).
- **Relay Role**: `relay/10_contract_opaque.sh` demonstrates that the relay handles artifacts as opaque blobs, as intended by the Frozen Data Plane architecture.
- **Integrated Boundary**: `integrated/` suite proves that `pavctl`, `relay`, and `pavis` speak the same protocol (Long-poll, ETag, PVS) in a realistic topology.

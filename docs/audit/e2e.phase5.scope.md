# E2E Test Audit - Phase 5: E2E Scope & Cost Balance

- Audit Phase: Phase 5 (E2E Scope & Cost Balance)
- Target Module: E2E
- Generation Timestamp: 2026-01-10T06:20:00Z
- AI Model Identifier: Gemini 2.0 Flash

## 1. Redundancy Analysis

- **Relay Monotonicity**: The `relay/11_contract_republish.sh` test duplicates unit tests in `crates/pavis-relay/src/state/tests.rs`. However, it validates that the HTTP 409 Conflict code is correctly mapped through the handlers, which is a valuable system-level proof.
- **Routing**: `pavis/40_traffic_matcher.sh` overlaps with unit tests in `crates/pavis/src/router/tests.rs`. This redundancy is acceptable as it verifies the integration between the `rkyv` deserializer and the active matching engine.

## 2. Missing Critical Scenarios

- **Multiple Listeners**: Most tests use a single port. There is no E2E test for multi-port binding/reloading consistency.
- **Upstream Health Check**: While planned in the roadmap, there is currently no implemented E2E scenario for outlier detection or passive health check propagation.
- **Persistence Storage Failure**: Relay tests verify persistence recovery, but not the behavior when the filesystem is read-only or full.

## 3. Cost Signals

- **Speed**:
  - `binary` mode: Extremely fast (~15-20s for full suite).
  - `docker` mode: Significantly slower due to container management overhead (~1.5 - 2m).
- **Parallelism**:
  - **Sequential Runner**: `tests/run.sh` executes tests one-by-one.
  - **Isolation Capability**: The scripts are designed for parallel safety (dynamic ports, unique temp dirs), but the runner does not yet exploit this.
- **Maintenance**:
  - The shell-based approach is easy to modify without recompiling a test runner, reducing the DX friction for adding new cases.

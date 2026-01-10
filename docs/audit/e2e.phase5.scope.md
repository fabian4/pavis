# E2E Test Audit - Phase 5: E2E Scope & Cost Balance

- Audit Phase: Phase 5 (E2E Scope & Cost Balance)
- Target Module: E2E
- Generation Timestamp: 2026-01-10T05:40:00Z
- AI Model Identifier: Gemini 2.0 Flash

## 1. Redundancy Analysis

### 1.1 Overlap with Integration Tests
There is significant functional overlap between crate-level tests and E2E scripts:
- **Relay Logic**: `pavis-relay` includes `http_tests.rs` which verifies publish/subscribe and monotonicity. The `relay` E2E suite (`10_contract_opaque.sh`, `11_contract_republish.sh`) repeats these checks. However, this is a **valuable redundancy** as the crate tests use `tower::Service` mocks, while the E2E tests exercise the real compiled binary and actual network stack.
- **PVS Generation**: `pavctl` has `gen_validation.rs`. E2E tests implicitly verify this in every case.

### 1.2 Suite Cross-Pollution
The `integrated` suite duplicates several scenarios from the `pavis` suite (e.g., traffic shifting). This is intentional to provide a "Full System Path" sanity check, but the `integrated` suite should remain minimal to avoid bloat.

## 2. Missing Critical Scenarios

The following high-risk areas were identified as lacking E2E coverage:

### 2.1 Control Plane Resilience
While `norestart` evolution is well-tested, the behavior of the `pavis` runtime when the `pavis-relay` is unreachable for an extended period is only planned (`integrated/40_resilience_restart.sh`) and not currently implemented.

### 2.2 Boundary Limits
Negative testing for resource limits (e.g., maximum PVS artifact size) is planned but currently skipped (`relay/70_limits_oversize.sh`). This leaves a gap in resource-protection verification.

### 2.3 Large-Scale Configuration
The current tests use very small configurations (1-2 listeners/upstreams). There is no E2E verification of system behavior when handling artifacts with hundreds of routes, which may expose performance regressions in the `rkyv` deserialization or the `Proxy` update logic.

## 3. Cost Signals

### 3.1 Execution Time
- **Binary Mode**: Highly efficient. 26 tests execute in ~30-40 seconds.
- **Docker Mode**: Significant overhead. The startup and teardown of containerized infrastructure increases execution time by ~5x compared to binary mode.

### 3.2 Parallelization Potential
The current `tests/run.sh` executes all cases sequentially. 
- **Signal**: As the suite grows, this will become a primary CI bottleneck.
- **Evidence**: The architectural use of `get_free_port` and `TEST_TMP` makes the suite **perfectly suited for parallelization**, but the current runner does not exploit this.

### 3.3 Infrastructure Weight
The reliance on `docker run --network host` for SUT components in Docker mode makes the suite difficult to run in restrictive environments (e.g., some DinD CI runners) where host networking is prohibited.
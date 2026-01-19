# Pavis Test Suite Evolution Plan

This plan evolves the Runtime, Relay, and Integrated suites to improve
version validation, LKG semantics, concurrency/load coverage, and semantic
validation coverage.

## Assumptions

- Runtime config version is observable via metrics (`pavis_runtime_config_version`).
- Runtime suite LKG test is `tests/suites/pavis/30_lkg.sh`.
- Legacy names like `20_reload_norestart` or `21_reload_zero_option_impact`
  may already be consolidated into `tests/suites/pavis/20_reload_contract_core.sh`.

## Recent Updates (Implemented)

- Added runtime env rejection tests:
  - `tests/suites/pavis/34_runtime_env_rejection.sh`
  - `tests/suites/integrated/32_runtime_env_rejection.sh`
- Updated `tests/suites/pavis/33_semantic_validation_matrix.sh` to treat missing CA bundle
  as a runtime env failure (`reason="runtime"`).
- Updated suite design docs to reflect runtime env validation coverage.

## Short-Term Actions (Priority)

### Runtime Suite (Data Plane)

0) Runtime env rejection coverage (DONE).
   - `tests/suites/pavis/34_runtime_env_rejection.sh` validates runtime env failures and LKG preservation.

1) Add explicit version validation to LKG.
   - Update `tests/suites/pavis/30_lkg.sh` to:
     - Fetch relay `x-config-version` after publishing corrupt/incompatible artifacts.
     - Scrape runtime metrics and assert `pavis_runtime_config_version` unchanged.
     - Keep existing traffic-based LKG assertions.

2) Confirm reload coverage consolidation.
   - Verify any legacy `20_reload_norestart` / `21_reload_zero_option_impact`
     coverage is present in `tests/suites/pavis/20_reload_contract_core.sh`.
   - If legacy files exist, merge missing assertions, then remove duplicates.

### Relay Suite

3) Long-poll liveness proof before publish.
   - Update `tests/suites/relay/20_longpoll_wait.sh` to:
     - Start long-poll in background.
     - Assert process still alive after short delay or use metrics-based readiness.

4) Strict monotonicity in concurrency test.
   - Update `tests/suites/relay/40_concurrency_rapid.sh` to:
     - Record all observed versions from `x-config-version`.
     - Assert strictly increasing sequence across all observed 200 responses.

### Integrated Suite

4) Runtime env rejection coverage (DONE).
   - `tests/suites/integrated/32_runtime_env_rejection.sh` validates end-to-end runtime env failures.

5) LKG version checks in end-to-end test.
   - Update `tests/suites/integrated/30_lkg_artifact.sh` to:
     - Enable runtime metrics for the test config.
     - Assert relay version > runtime version after bad artifact publish.
     - Keep traffic-based LKG assertions.

## Mid-Term Actions

### Integrated Suite

6) Concurrency during reload.
   - Enhance `tests/suites/integrated/20_reload_switch.sh` with a 200-request
     burst during V1 -> V2 transition, mirroring runtime test logic.
   - Assert zero failures and no version regression within the burst.

7) Multi-version chain test.
   - Add a new test (e.g. `tests/suites/integrated/50_multiversion_chain.sh`) to:
     - Publish V1 -> V2 -> V3 -> V4 rapidly.
     - Validate relay versions advance and runtime applies in-order.

### Relay Suite

8) Optional: long-poll metrics consistency under churn.
   - Cross-check `pavis_relay_longpoll_wait_total` vs. actual subscriber count.

## Long-Term Actions

9) Semantic rejection in integrated suite.
   - Enable `tests/suites/integrated/31_lkg_rejection.sh` once runtime implements
     semantic validation before apply.
   - Use deterministic semantic errors (e.g., route references unknown upstream).

10) Network partition and failover simulation.
   - Add integrated tests for relay unreachability and recovery behavior.

## Cross-Cutting Helpers

- Add shared helper(s) in `tests/scripts/assert.sh` to:
  - Extract relay `x-config-version`.
  - Scrape runtime `pavis_runtime_config_version`.
  - Compare relay/runtime versions in LKG tests.

## Validation

- After suite edits:
  - `tests/run.sh pavis`
  - `tests/run.sh relay`
  - `tests/run.sh integrated`
- If Rust code changes are required:
  - `make ci-local`

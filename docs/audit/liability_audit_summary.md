# Repository Liability Audit Summary

## Audit Scope & Rules
- 
- Code is treated as liability by default; evidence only.
- Scan is file-by-file, crate-by-crate, then tests/ and bench/.
- No refactors, redesigns, or recommendations are included.

## Audit Coverage
- Paths scanned: crates/pavis, crates/pavctl, crates/pavis-pvs, crates/pavis-core, crates/pavis-codec-api, crates/pavis-codec-serde, crates/pavis-ingest-api, crates/pavis-ingest-file, crates/pavis-relay, crates/pavis-testkit, crates/pavis-benchkit, tests/, bench/
- Total files scanned per unit:
  - crates/pavis: 37
  - crates/pavctl: 13
  - crates/pavis-pvs: 8
  - crates/pavis-core: 17
  - crates/pavis-codec-api: 3
  - crates/pavis-codec-serde: 26
  - crates/pavis-ingest-api: 3
  - crates/pavis-ingest-file: 4
  - crates/pavis-relay: 20
  - crates/pavis-testkit: 29
  - crates/pavis-benchkit: 7
  - tests: 48
  - bench: 227
- Paths skipped: None

## Workspace Map
### Rust Crates (Audit Order)
- crates/pavis - description: "Pavis runtime"
- crates/pavctl - description: "Pavis CLI"
- crates/pavis-pvs - description: "PVS file protocol boundary for Pavis"
- crates/pavis-core - description: "Core primitives and engine for the Pavis data plane"
- crates/pavis-codec-api - description: "Codec API traits for Pavis"
- crates/pavis-codec-serde - description: "Serde-based codecs for Pavis"
- crates/pavis-ingest-api - description: "Ingest API types for Pavis"
- crates/pavis-ingest-file - description missing in Cargo.toml
- crates/pavis-relay - description: "Relay layer for Pavis"
- crates/pavis-testkit - description missing in Cargo.toml
- crates/pavis-benchkit - description missing in Cargo.toml

### Special Modules
- E2E: tests/
- Bench: bench/

## Crate Audits
### pavis
#### Inventory
- Files scanned: 37
- Key modules/files: crates/pavis/src/proxy/service.rs, crates/pavis/src/proxy/context.rs, crates/pavis/src/agent/worker/agent.rs, crates/pavis/src/telemetry/tracing.rs

#### Liability Ledger
- F-pavis-1
  - Gate: Change-Resilience
  - Severity: High
  - Evidence:
    - crates/pavis/src/proxy/service.rs + generate_request_id + "SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()"
  - Impact: Clock underflow triggers panic in request path, terminating the process.
  - Confidence: High
- F-pavis-2
  - Gate: Intent
  - Severity: High
  - Evidence:
    - crates/pavis/src/proxy/service.rs + upstream_peer + "Runtime state missing from context; using latest snapshot" and "self.state.load()"
  - Impact: A request can mix route selection and upstream selection across snapshots.
  - Confidence: High
- F-pavis-3
  - Gate: Verifiability
  - Severity: Medium
  - Evidence:
    - crates/pavis/src/proxy/context.rs + RequestId::as_str + "unsafe { std::str::from_utf8_unchecked(&self.buf[..len]) }"
  - Impact: Invalid UTF-8 in request id buffer is unchecked and can produce undefined behavior.
  - Confidence: High
- F-pavis-4
  - Gate: Change-Resilience
  - Severity: Medium
  - Evidence:
    - crates/pavis/src/agent/worker/agent.rs + on_update_callback + "self.on_update_callback.lock().unwrap()"
  - Impact: Lock poisoning causes panic during update callbacks.
  - Confidence: High

#### Verdict
- Justified liabilities: 4
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavctl
#### Inventory
- Files scanned: 13
- Key modules/files: crates/pavctl/src/commands.rs, crates/pavctl/src/main.rs, crates/pavctl/tests/pipeline.rs

#### Liability Ledger
- F-pavctl-1
  - Gate: Verifiability
  - Severity: Low
  - Evidence:
    - crates/pavctl/src/commands.rs + unique_path + "SystemTime::now().duration_since(UNIX_EPOCH).expect(\"time\")"
  - Impact: Test helper panics on clock underflow and aborts test runs.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavis-pvs
#### Inventory
- Files scanned: 8
- Key modules/files: crates/pavis-pvs/src/read.rs, crates/pavis-pvs/src/verify.rs, crates/pavis-pvs/src/write.rs

#### Liability Ledger
- F-pavis-pvs-1
  - Gate: Verifiability
  - Severity: Medium
  - Evidence:
    - crates/pavis-pvs/src/read.rs + parse_header + "buf[0..4].try_into().unwrap()"
  - Impact: If header length checks are bypassed or changed, parsing panics.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavis-core
#### Inventory
- Files scanned: 17
- Key modules/files: crates/pavis-core/src/validate/routes.rs, crates/pavis-core/src/lib.rs, crates/pavis-core/src/types.rs

#### Liability Ledger
- F-pavis-core-1
  - Gate: Change-Resilience
  - Severity: Medium
  - Evidence:
    - crates/pavis-core/src/validate/routes.rs + regex cache + "expect(\"regex cache lock poisoned\")"
  - Impact: Lock poisoning turns a prior panic into a new panic during route validation.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavis-codec-api
#### Inventory
- Files scanned: 3
- Key modules/files: crates/pavis-codec-api/src/lib.rs

#### Liability Ledger
- F-pavis-codec-api-1
  - Gate: Verifiability
  - Severity: Medium
  - Evidence:
    - crates/pavis-codec-api/src/lib.rs + CheckedArtifact + "state: Option<Arc<dyn Any + Send + Sync>>"
  - Impact: Type-erased state requires downcasts at runtime and can fail without compile-time checks.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavis-codec-serde
#### Inventory
- Files scanned: 26
- Key modules/files: crates/pavis-codec-serde/src/config/convert/routes.rs, crates/pavis-codec-serde/src/config/types.rs

#### Liability Ledger
- F-pavis-codec-serde-1
  - Gate: Change-Resilience
  - Severity: High
  - Evidence:
    - crates/pavis-codec-serde/src/config/convert/routes.rs + to_runtime + "panic!(\"unknown route action variant\")"
  - Impact: Unexpected enum variants crash conversion instead of returning an error.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavis-ingest-api
#### Inventory
- Files scanned: 3
- Key modules/files: crates/pavis-ingest-api/src/lib.rs

#### Liability Ledger
- F-pavis-ingest-api-1
  - Gate: Verifiability
  - Severity: Low
  - Evidence:
    - crates/pavis-ingest-api/src/lib.rs + Artifact::new + "received_at: SystemTime::now()"
  - Impact: Creation time is non-deterministic and affects test repeatability.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavis-ingest-file
#### Inventory
- Files scanned: 4
- Key modules/files: crates/pavis-ingest-file/src/lib.rs

#### Liability Ledger
- F-pavis-ingest-file-1
  - Gate: Verifiability
  - Severity: Medium
  - Evidence:
    - crates/pavis-ingest-file/src/lib.rs + read_artifact + "tokio::fs::read(&self.path).await"
  - Impact: Reads entire artifact into memory without size cap.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavis-relay
#### Inventory
- Files scanned: 20
- Key modules/files: crates/pavis-relay/src/app.rs, crates/pavis-relay/src/handlers.rs, crates/pavis-relay/src/state.rs

#### Liability Ledger
- F-pavis-relay-1
  - Gate: Change-Resilience
  - Severity: Medium
  - Evidence:
    - crates/pavis-relay/src/app.rs + init_state + "std::fs::read(&lkg_path)"
  - Impact: LKG file is read fully into memory without an explicit size cap.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavis-testkit
#### Inventory
- Files scanned: 29
- Key modules/files: crates/pavis-testkit/src/relay/routes/longpoll.rs, crates/pavis-testkit/src/bin/pavis-mock-relay.rs

#### Liability Ledger
- F-pavis-testkit-1
  - Gate: Verifiability
  - Severity: Medium
  - Evidence:
    - crates/pavis-testkit/src/relay/routes/longpoll.rs + handler + "meta.etag.parse().unwrap()"
  - Impact: Malformed metadata causes testkit process panic.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavis-benchkit
#### Inventory
- Files scanned: 7
- Key modules/files: crates/pavis-benchkit/src/bin/bench-upstream.rs, crates/pavis-benchkit/src/bin/bench-loadgen.rs

#### Liability Ledger
- F-pavis-benchkit-1
  - Gate: Intent
  - Severity: Low
  - Evidence:
    - crates/pavis-benchkit/src/bin/bench-upstream.rs + parse_env_u16 + "and_then(|value| value.parse().ok()).unwrap_or(default)"
  - Impact: Invalid environment values are silently replaced with defaults.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

## E2E Audit

### Inventory
- Total suites: 3
- Total cases: 36
- Runner and shared libraries: tests/run.sh, tests/scripts/env.sh, tests/scripts/assert.sh, tests/scripts/docker.sh, tests/scripts/log.sh

### Case Ledger
- E2E-01
  - File: tests/suites/integrated/10_bootstrap_path.sh
  - Scenario & invariant under test: Full path bootstrap and publish; invariants I1 and I2.
  - Inputs (config, traffic, env): relay.yaml, bootstrap.yaml, config_v1.yaml; curl publish to /v1/publish; upstream ports from env.
  - Assertions (exact checks): assert_status 404 before publish; poll /echo for success.
  - Evidence (assert/curl/log snippet): tests/suites/integrated/10_bootstrap_path.sh:49 assert_status ".../echo" 404
  - Determinism risks (timing, ports, retries, env): get_free_port and sleep 0.5 loops with MAX_RETRIES=20.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): runs relay and runtime processes and verifies live HTTP publish and traffic.
- E2E-02
  - File: tests/suites/integrated/20_reload_switch.sh
  - Scenario & invariant under test: Traffic shifts from backend-v1 to backend-v2 without restart.
  - Inputs (config, traffic, env): relay.yaml, config_v1.yaml, config_v2.yaml; curl publish v1/v2.
  - Assertions (exact checks): backend-v1 match; poll for backend-v2; SUT identity unchanged.
  - Evidence (assert/curl/log snippet): tests/suites/integrated/20_reload_switch.sh:112 "Traffic did not switch to backend-v2"
  - Determinism risks (timing, ports, retries, env): sleep 0.5 loops with MAX_RETRIES=20; get_free_port.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): reload path depends on relay publish and runtime live traffic.
- E2E-03
  - File: tests/suites/integrated/21_reload_stable.sh
  - Scenario & invariant under test: Idempotent update with stable traffic; invariant I2.
  - Inputs (config, traffic, env): relay.yaml, config.yaml; publish version 1 and 2 (same content).
  - Assertions (exact checks): assert_body contains backend-v1; loop checks response stability.
  - Evidence (assert/curl/log snippet): tests/suites/integrated/21_reload_stable.sh:72 "Traffic failure during idempotent update"
  - Determinism risks (timing, ports, retries, env): 20-iteration loop with sleep 0.1.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): requires runtime and relay interaction while serving traffic.
- E2E-04
  - File: tests/suites/integrated/30_lkg_artifact.sh
  - Scenario & invariant under test: LKG behavior with corrupt publish and recovery; invariants I3 and I4.
  - Inputs (config, traffic, env): relay.yaml, config_v1.yaml, corrupt.pvs, config_v3.yaml; curl publish.
  - Assertions (exact checks): assert_body backend-v1 before and after corrupt publish; switch to v3.
  - Evidence (assert/curl/log snippet): tests/suites/integrated/30_lkg_artifact.sh:76 assert_body ".../echo" "backend-v1"
  - Determinism risks (timing, ports, retries, env): sleep 2; MAX_RETRIES=20 with sleep 0.5.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): validates relay publish and runtime LKG behavior on live HTTP.
- E2E-05
  - File: tests/suites/integrated/31_lkg_rejection.sh
  - Scenario & invariant under test: Skipped test for semantic rejection; invariant I4.
  - Inputs (config, traffic, env): none (skipped).
  - Assertions (exact checks): exit 77.
  - Evidence (assert/curl/log snippet): tests/suites/integrated/31_lkg_rejection.sh:7 "Skipping lkg_02_semantic_rejection"; exit 77
  - Determinism risks (timing, ports, retries, env): none (skipped).
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): not executed; script exits before setup.
- E2E-06
  - File: tests/suites/integrated/40_resilience_restart.sh
  - Scenario & invariant under test: Relay restart with LKG continuity; invariants I2 and I4.
  - Inputs (config, traffic, env): relay.yaml, bootstrap.yaml, config_v1.yaml, config_v2.yaml; curl publish.
  - Assertions (exact checks): assert_body backend-v1 during relay down; poll for backend-v2 after restart.
  - Evidence (assert/curl/log snippet): tests/suites/integrated/40_resilience_restart.sh:98 "Runtime did not pick up V2 after Relay restart"
  - Determinism risks (timing, ports, retries, env): sleep 0.1/0.2 loops; MAX_RETRIES=50; get_free_port.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): exercises relay process restart and runtime behavior.

- E2E-07
  - File: tests/suites/pavis/10_bootstrap_static.sh
  - Scenario & invariant under test: Static bootstrap without relay; invariant D.
  - Inputs (config, traffic, env): initial.yaml; HTTP /echo request.
  - Assertions (exact checks): assert_json_has_key instance_id; backend-v1 match.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/10_bootstrap_static.sh:53 "Expected backend-v1, got $instance"
  - Determinism risks (timing, ports, retries, env): get_free_port; wait_for_url.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): validates runtime binary behavior and upstream routing.
- E2E-08
  - File: tests/suites/pavis/20_reload_norestart.sh
  - Scenario & invariant under test: Hot reload with zero-drop and atomic switch; invariants A and C.
  - Inputs (config, traffic, env): mock relay; config_v1.yaml, config_v2.yaml; burst traffic.
  - Assertions (exact checks): no FAIL in burst; no V1 after V2; SUT identity constant.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/20_reload_norestart.sh:121 "Non-atomic switch detected"
  - Determinism risks (timing, ports, retries, env): concurrent burst loop; sleep 0.1.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): requires live reload with concurrent traffic.
- E2E-09
  - File: tests/suites/pavis/21_reload_zero_option_impact.sh
  - Scenario & invariant under test: Zero-option behavior when header policy removed; invariant D.
  - Inputs (config, traffic, env): mock relay; config_v1.yaml, config_v2.yaml; curl -I.
  - Assertions (exact checks): header present in v1; header absent after reload.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/21_reload_zero_option_impact.sh:72 "Invariant D violated"
  - Determinism risks (timing, ports, retries, env): sleep 0.5 loops; MAX_RETRIES=20.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): validates runtime behavior across reload with live headers.
- E2E-10
  - File: tests/suites/pavis/30_lkg.sh
  - Scenario & invariant under test: LKG guardrails with corrupt and incompatible artifacts; invariant B.
  - Inputs (config, traffic, env): mock relay; corrupt.pvs; config_v1/v2/v3.yaml.
  - Assertions (exact checks): backend remains v1 after corrupt and incompatible; switch to v3; process alive.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/30_lkg.sh:125 "Recovery failed: Runtime did not switch"
  - Determinism risks (timing, ports, retries, env): sleep 2; MAX_RETRIES=20.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): exercises runtime LKG handling with relay publish.
- E2E-11
  - File: tests/suites/pavis/40_traffic_routing_semantics.sh
  - Scenario & invariant under test: Route matching, header policies, redirects, direct response, rewrite; invariants C and D.
  - Inputs (config, traffic, env): mock relay; config.yaml; multiple curl requests and headers.
  - Assertions (exact checks): exact/prefix/regex match, header set/append/remove, redirect status/location, rewrite behavior.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/40_traffic_routing_semantics.sh:87 "Exact route did not win"
  - Determinism risks (timing, ports, retries, env): relies on live upstream responses and multiple curl calls.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): uses runtime network routing and header mutation behavior.
- E2E-12
  - File: tests/suites/pavis/41_traffic_weighted.sh
  - Scenario & invariant under test: Weighted routing flip across reloads; invariant A.
  - Inputs (config, traffic, env): mock relay; config_v1/v2/v3.yaml; repeated /echo requests.
  - Assertions (exact checks): only backend-v1 then only backend-v2 then only backend-v1.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/41_traffic_weighted.sh:130 "Traffic did not shift to backend-v2"
  - Determinism risks (timing, ports, retries, env): MAX_RETRIES=20 with sleep 0.5; multiple loops.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): validates runtime routing under live reload and traffic.
- E2E-13
  - File: tests/suites/pavis/50_resilience_timeout.sh
  - Scenario & invariant under test: Timeout tightening after reload; route timeout enforced.
  - Inputs (config, traffic, env): mock relay; config_v1 timeout 500ms; config_v2 timeout 50ms; `/delay?ms=100` then `/delay?ms=200`.
  - Assertions (exact checks): v1 delay returns 200; v2 delay fails quickly after reload.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/50_resilience_timeout.sh:30 "assert_status ... /delay?ms=100 200"
  - Determinism risks (timing, ports, retries, env): polling loop with MAX_RETRIES=20 and timing threshold.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): requires live reload + upstream latency to confirm timeout enforcement.
- E2E-14
  - File: tests/suites/pavis/51_resilience_retry.sh
  - Scenario & invariant under test: Retry policy with connect_failure succeeds via fallback endpoint.
  - Inputs (config, traffic, env): dead endpoint + healthy endpoint; retry_on connect_failure; `/echo`.
  - Assertions (exact checks): response instance_id == "backend-v1".
  - Evidence (assert/curl/log snippet): tests/suites/pavis/51_resilience_retry.sh:44 "Expected retry to reach backend-v1"
  - Determinism risks (timing, ports, retries, env): one request; depends on retry path hitting healthy endpoint.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): validates retry behavior across real network connection failures.
- E2E-15
  - File: tests/suites/pavis/60_security_tls.sh
  - Scenario & invariant under test: Skipped TLS origination toggle test.
  - Inputs (config, traffic, env): none (skipped).
  - Assertions (exact checks): exit 0.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/60_security_tls.sh:11 "SKIPPED"; exit 0
  - Determinism risks (timing, ports, retries, env): none (skipped).
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): not executed; script exits before setup.
- E2E-16
  - File: tests/suites/pavis/61_security_inbound_mtls.sh
  - Scenario & invariant under test: Skipped inbound mTLS test.
  - Inputs (config, traffic, env): none (skipped).
  - Assertions (exact checks): exit 0.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/61_security_inbound_mtls.sh:11 "SKIPPED"; exit 0
  - Determinism risks (timing, ports, retries, env): none (skipped).
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): not executed; script exits before setup.

- E2E-17
  - File: tests/suites/pavis/63_security_rbac_spiffe.sh
  - Scenario & invariant under test: Skipped RBAC SPIFFE test.
  - Inputs (config, traffic, env): none (skipped).
  - Assertions (exact checks): exit 0.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/63_security_rbac_spiffe.sh:2-3 "Skipped RBAC"; exit 0
  - Determinism risks (timing, ports, retries, env): none (skipped).
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): not executed; script exits before setup.
- E2E-18
  - File: tests/suites/pavis/64_security_rbac_prefix.sh
  - Scenario & invariant under test: Skipped RBAC prefix test.
  - Inputs (config, traffic, env): none (skipped).
  - Assertions (exact checks): exit 0.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/64_security_rbac_prefix.sh:2-3 "Skipped RBAC"; exit 0
  - Determinism risks (timing, ports, retries, env): none (skipped).
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): not executed; script exits before setup.
- E2E-19
  - File: tests/suites/pavis/65_security_mtls_outbound.sh
  - Scenario & invariant under test: Skipped outbound mTLS test.
  - Inputs (config, traffic, env): none (skipped).
  - Assertions (exact checks): exit 0.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/65_security_mtls_outbound.sh:11 "SKIPPED"; exit 0
  - Determinism risks (timing, ports, retries, env): none (skipped).
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): not executed; script exits before setup.
- E2E-20
  - File: tests/suites/pavis/66_security_tls_sni_auto.sh
  - Scenario & invariant under test: Skipped TLS SNI auto test.
  - Inputs (config, traffic, env): none (skipped).
  - Assertions (exact checks): exit 0.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/66_security_tls_sni_auto.sh:11 "SKIPPED"; exit 0
  - Determinism risks (timing, ports, retries, env): none (skipped).
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): not executed; script exits before setup.
- E2E-21
  - File: tests/suites/pavis/67_security_mtls_chain_mode.sh
  - Scenario & invariant under test: Skipped mTLS chain mode test.
  - Inputs (config, traffic, env): none (skipped).
  - Assertions (exact checks): exit 0.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/67_security_mtls_chain_mode.sh:11 "SKIPPED"; exit 0
  - Determinism risks (timing, ports, retries, env): none (skipped).
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): not executed; script exits before setup.
- E2E-22
  - File: tests/suites/pavis/70_obs_metrics.sh
  - Scenario & invariant under test: Metrics emission and cardinality guardrails; invariant D.
  - Inputs (config, traffic, env): config.yaml with metrics; HTTP /echo; mock relay for reload.
  - Assertions (exact checks): counters equal expected values; unmatched paths not in metrics; counters persist after reload.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/70_obs_metrics.sh:61 "Metrics missing or incorrect count"
  - Determinism risks (timing, ports, retries, env): uses date for unmatched paths; sleep 2 after reload.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): validates metrics endpoint and reload behavior in runtime.
- E2E-23
  - File: tests/suites/pavis/71_obs_access_log.sh
  - Scenario & invariant under test: Skipped access log test.
  - Inputs (config, traffic, env): none (skipped).
  - Assertions (exact checks): exit 77.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/71_obs_access_log.sh:2-3 "Skipping"; exit 77
  - Determinism risks (timing, ports, retries, env): none (skipped).
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): not executed; script exits before setup.
- E2E-24
  - File: tests/suites/pavis/72_obs_tracing_context.sh
  - Scenario & invariant under test: Skipped tracing context test.
  - Inputs (config, traffic, env): none (skipped).
  - Assertions (exact checks): exit 77.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/72_obs_tracing_context.sh:2-3 "Skipping"; exit 77
  - Determinism risks (timing, ports, retries, env): none (skipped).
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): not executed; script exits before setup.
- E2E-25
  - File: tests/suites/pavis/80_obs_cross_consistency.sh
  - Scenario & invariant under test: Skipped cross-consistency test.
  - Inputs (config, traffic, env): none (skipped).
  - Assertions (exact checks): exit 77.
  - Evidence (assert/curl/log snippet): tests/suites/pavis/80_obs_cross_consistency.sh:2-3 "Skipping"; exit 77
  - Determinism risks (timing, ports, retries, env): none (skipped).
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): not executed; script exits before setup.

- E2E-26
  - File: tests/suites/relay/10_contract_opaque.sh
  - Scenario & invariant under test: Opaque publish/subscribe contract; invariants R1 and R2.
  - Inputs (config, traffic, env): relay.yaml; gen_minimal_pvs; curl publish and config fetch.
  - Assertions (exact checks): status 200 on publish and config; body equals published bytes; header version equals 1.
  - Evidence (assert/curl/log snippet): tests/suites/relay/10_contract_opaque.sh:49 "Body mismatch"
  - Determinism risks (timing, ports, retries, env): get_free_port; external process.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): validates live HTTP publish/subscribe and byte integrity.
- E2E-27
  - File: tests/suites/relay/11_contract_republish.sh
  - Scenario & invariant under test: Idempotent republish with version increment; invariants R1 and R5.
  - Inputs (config, traffic, env): relay.yaml; payload.pvs; curl publish v1 and v2.
  - Assertions (exact checks): status 200; body equals payload; header version equals 2.
  - Evidence (assert/curl/log snippet): tests/suites/relay/11_contract_republish.sh:50 "Body mismatch"
  - Determinism risks (timing, ports, retries, env): get_free_port; external process.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): tests relay HTTP API correctness with live storage.
- E2E-28
  - File: tests/suites/relay/20_longpoll_wait.sh
  - Scenario & invariant under test: Long-poll waits for update; invariants R3 and R2.
  - Inputs (config, traffic, env): relay.yaml with long_poll enabled; publish v1 and v2; long-poll request.
  - Assertions (exact checks): subscriber blocks then returns 200 with v2 body.
  - Evidence (assert/curl/log snippet): tests/suites/relay/20_longpoll_wait.sh:61 "Subscriber exited prematurely"
  - Determinism risks (timing, ports, retries, env): sleep 0.5; wait_ms=5000; background process.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): requires long-poll behavior across HTTP.
- E2E-29
  - File: tests/suites/relay/21_longpoll_timeout.sh
  - Scenario & invariant under test: Long-poll timeout returns 304; invariant R3.
  - Inputs (config, traffic, env): relay.yaml with long_poll enabled; publish v1; long-poll wait_ms=2000.
  - Assertions (exact checks): HTTP 304 and duration >= 2 seconds.
  - Evidence (assert/curl/log snippet): tests/suites/relay/21_longpoll_timeout.sh:48 "Expected 304"
  - Determinism risks (timing, ports, retries, env): timing depends on system clock and sleep.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): validates long-poll HTTP behavior.
- E2E-30
  - File: tests/suites/relay/30_fanout_multi.sh
  - Scenario & invariant under test: Fanout to multiple subscribers; invariant R4.
  - Inputs (config, traffic, env): relay.yaml with long_poll enabled; publish v1 and v2; 5 subscribers.
  - Assertions (exact checks): subscribers register in metrics; all return 200.
  - Evidence (assert/curl/log snippet): tests/suites/relay/30_fanout_multi.sh:65 "Subscribers did not register"
  - Determinism risks (timing, ports, retries, env): metrics polling loop with sleep 0.1; background processes.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): checks fanout via live HTTP and metrics endpoint.
- E2E-31
  - File: tests/suites/relay/31_fanout_late.sh
  - Scenario & invariant under test: Late subscriber catches up immediately; invariant R2.
  - Inputs (config, traffic, env): publish v5; request with old version.
  - Assertions (exact checks): HTTP 200 and no blocking (duration < 2s).
  - Evidence (assert/curl/log snippet): tests/suites/relay/31_fanout_late.sh:48 "Request blocked unexpectedly"
  - Determinism risks (timing, ports, retries, env): timing check on duration; system clock.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): verifies delivery semantics over HTTP.
- E2E-32
  - File: tests/suites/relay/40_concurrency_rapid.sh
  - Scenario & invariant under test: Rapid publish/subscribe concurrency; invariants R5 and R2.
  - Inputs (config, traffic, env): 50 publishes; subscriber loop with wait_ms=100.
  - Assertions (exact checks): no version regression; final version equals 50; relay health ok.
  - Evidence (assert/curl/log snippet): tests/suites/relay/40_concurrency_rapid.sh:60 "Version regression detected"
  - Determinism risks (timing, ports, retries, env): concurrent publisher/subscriber loops; timeouts.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): concurrency behavior depends on live relay state.
- E2E-33
  - File: tests/suites/relay/50_persistence_recovery.sh
  - Scenario & invariant under test: File storage persistence across restart; invariant R6.
  - Inputs (config, traffic, env): relay.yaml with storage type file; publish pvs; restart relay.
  - Assertions (exact checks): served data matches after restart.
  - Evidence (assert/curl/log snippet): tests/suites/relay/50_persistence_recovery.sh:57 "Data lost after restart"
  - Determinism risks (timing, ports, retries, env): filesystem IO; process restart.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): persistence depends on process lifecycle and storage.
- E2E-34
  - File: tests/suites/relay/60_robustness_reconnect.sh
  - Scenario & invariant under test: Subscriber reconnect after disconnect; invariants R2 and R3.
  - Inputs (config, traffic, env): long-poll with max-time 1; publish v2; reconnect.
  - Assertions (exact checks): body equals v2; request not blocked.
  - Evidence (assert/curl/log snippet): tests/suites/relay/60_robustness_reconnect.sh:66 "Request blocked unexpectedly"
  - Determinism risks (timing, ports, retries, env): timing with max-time and duration checks.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): relies on HTTP long-poll behavior and reconnection.
- E2E-35
  - File: tests/suites/relay/70_limits_oversize.sh
  - Scenario & invariant under test: Publish size limit enforced; invariant R7.
  - Inputs (config, traffic, env): relay.yaml with max_pvs_bytes 100; publish PVS.
  - Assertions (exact checks): HTTP 413 response.
  - Evidence (assert/curl/log snippet): tests/suites/relay/70_limits_oversize.sh:61 assert_status_eq 413
  - Determinism risks (timing, ports, retries, env): file size check uses stat.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): enforces HTTP limits on relay endpoint.
- E2E-36
  - File: tests/suites/relay/71_limits_empty.sh
  - Scenario & invariant under test: Empty publish rejected; invariant R1.
  - Inputs (config, traffic, env): relay.yaml; empty body; publish.
  - Assertions (exact checks): status 400 or 422; not 200.
  - Evidence (assert/curl/log snippet): tests/suites/relay/71_limits_empty.sh:43 "Unexpected success for empty body"
  - Determinism risks (timing, ports, retries, env): none beyond port selection.
  - Failure signal quality (clear / ambiguous / noisy): clear
  - Why E2E (why unit/integration is insufficient): validates live HTTP input validation.

### Verdict
- High-signal cases: 12
- Fragile/noisy cases: 11
- Questionable-existence cases: 13

## Bench Audit

### Inventory
- Total cases: 9
- Tooling and runners: bench/run.sh, bench/scripts/benchmark.sh, bench/scripts/requirements.sh, bench/scripts/summarize.sh

### Case Ledger
- BENCH-01
  - File: bench/cases/standalone/churn_short_1x.sh
  - Workload type (open-loop / closed-loop): closed-loop
  - Exact run command: wrk -t "$THREADS" -c "$CONNECTIONS" -d "${duration}s" -H "Connection: close" "$PROXY_URL"
  - Metrics produced (field names): Requests/sec, Socket errors, latency percentiles (50%, 99%), docker_stats.csv cpu_pct/mem_usage.
  - Reproducibility constraints (CPU, kernel, cgroup, warmup): docker compose required; wrk required; optional BENCH_LOADGEN_CPUSET.
  - Noise sources: docker stats sampled every 1s; background stats loop.
  - Classification: Regression Gate
- BENCH-02
  - File: bench/cases/standalone/concurrency_short_1x.sh
  - Workload type (open-loop / closed-loop): closed-loop
  - Exact run command: wrk -t "$THREADS" -c "$CONNECTIONS" -d "${duration}s" "$PROXY_URL"
  - Metrics produced (field names): Requests/sec, Socket errors, latency percentiles (50%, 99%), docker_stats.csv cpu_pct/mem_usage.
  - Reproducibility constraints (CPU, kernel, cgroup, warmup): docker compose required; wrk required; optional BENCH_LOADGEN_CPUSET.
  - Noise sources: docker stats sampled every 1s; background stats loop.
  - Classification: Regression Gate
- BENCH-03
  - File: bench/cases/standalone/latency_short_1x.sh
  - Workload type (open-loop / closed-loop): open-loop
  - Exact run command: bench-loadgen --url "$PROXY_URL" --rate "$TARGET_RPS" --duration "$duration" --connections "$CONNECTIONS"
  - Metrics produced (field names): achieved_rps, errors, latency_ms.p50, latency_ms.p99, dropped, docker_stats.csv cpu_pct/mem_usage.
  - Reproducibility constraints (CPU, kernel, cgroup, warmup): bench-loadgen required; docker compose; optional BENCH_LOADGEN_CPUSET.
  - Noise sources: docker stats sampled every 1s; warmup/cooldown sleeps.
  - Classification: Regression Gate
- BENCH-04
  - File: bench/cases/standalone/latency_extended_1x.sh
  - Workload type (open-loop / closed-loop): open-loop
  - Exact run command: bench-loadgen --url "$PROXY_URL" --rate "$TARGET_RPS" --duration "$duration" --connections "$CONNECTIONS"
  - Metrics produced (field names): achieved_rps, errors, latency_ms.p99, dropped, docker_stats.csv cpu_pct/mem_usage.
  - Reproducibility constraints (CPU, kernel, cgroup, warmup): bench-loadgen required; docker compose; RUN_COUNT iterations.
  - Noise sources: multiple runs with cooldown; docker stats sampling.
  - Classification: Exploratory
- BENCH-05
  - File: bench/cases/standalone/throughput_short_1x.sh
  - Workload type (open-loop / closed-loop): closed-loop
  - Exact run command: wrk -t "$THREADS" -c "$CONNECTIONS" -d "${duration}s" "$PROXY_URL"
  - Metrics produced (field names): Requests/sec per run, latency p99 per run, Socket errors, docker_stats.csv cpu_pct/mem_usage.
  - Reproducibility constraints (CPU, kernel, cgroup, warmup): docker compose required; wrk required; RUN_COUNT iterations.
  - Noise sources: multiple runs with cooldown; docker stats sampling.
  - Classification: Regression Gate

- BENCH-06
  - File: bench/cases/system/config_reload_convergence.sh
  - Workload type (open-loop / closed-loop): open-loop
  - Exact run command: bench-loadgen --url "$target_url" --rate "$TARGET_RPS" --duration "$DURATION_S" --connections 100
  - Metrics produced (field names): baseline_p99_ms, transition_p99_ms, convergence_time_ms, errors_5xx.
  - Reproducibility constraints (CPU, kernel, cgroup, warmup): Kubernetes kind cluster; kubectl port-forward; bench-loadgen.
  - Noise sources: port-forward latency; sleep 3; background system metrics.
  - Classification: Regression Gate
- BENCH-07
  - File: bench/cases/system/rollback_performance.sh
  - Workload type (open-loop / closed-loop): open-loop
  - Exact run command: bench-loadgen --url "$target_url" --rate "$TARGET_RPS" --duration "$BASELINE_DURATION_S" --connections 100
  - Metrics produced (field names): baseline_p99_ms, degraded_errors, rollback_ttbr_ms, recovery_p99_ms, recovery_errors.
  - Reproducibility constraints (CPU, kernel, cgroup, warmup): Kubernetes kind cluster; kubectl port-forward; bench-loadgen.
  - Noise sources: polling loop for TTBR; sleep 1; port-forward.
  - Classification: Regression Gate
- BENCH-08
  - File: bench/cases/system/stress_recovery.sh
  - Workload type (open-loop / closed-loop): open-loop
  - Exact run command: bench-loadgen --url "$target_url" --rate "$BASELINE_RPS" --duration "$BASELINE_DURATION_S" --connections 100
  - Metrics produced (field names): baseline_p99_ms, stress_p99_ms, recovery_p99_ms, stress_dropped, rss_growth_pct.
  - Reproducibility constraints (CPU, kernel, cgroup, warmup): Kubernetes kind cluster; kubectl port-forward; bench-loadgen.
  - Noise sources: background RSS timeline sampling; sleep 3; port-forward.
  - Classification: Regression Gate
- BENCH-09
  - File: bench/cases/system/multi_hour_soak.sh
  - Workload type (open-loop / closed-loop): open-loop
  - Exact run command: bench-loadgen --url "$target_url" --rate "$TARGET_RPS" --duration "$DURATION_S" --connections 150
  - Metrics produced (field names): achieved_rps, errors, p99_ms, rss_slope_mb_per_hour, fd_delta.
  - Reproducibility constraints (CPU, kernel, cgroup, warmup): Kubernetes kind cluster; long duration; bench-loadgen.
  - Noise sources: long duration, RSS and FD sampling every 60s.
  - Classification: Exploratory

### Verdict
- Gate benchmarks: 7
- Exploratory benchmarks: 2
- Invalid / misleading benchmarks: 0

## Systemic Liability Patterns
- Panic paths on time, lock, and enum mismatches: F-pavis-1, F-pavis-core-1, F-pavis-codec-serde-1, F-pavctl-1, F-pavis-testkit-1.
- Unbounded reads into memory: F-pavis-ingest-file-1, F-pavis-relay-1.
- Unsafe unchecked string assumptions in request id handling: F-pavis-3.
- Type-erased state in codec API: F-pavis-codec-api-1.

## Highest Compound-Risk Areas
1. Runtime request handling and snapshot selection: F-pavis-1, F-pavis-2, F-pavis-3, F-pavis-4.
2. Artifact ingestion and storage memory pressure: F-pavis-ingest-file-1, F-pavis-relay-1, F-pavis-ingest-api-1.
3. Config conversion and validation panics: F-pavis-core-1, F-pavis-codec-serde-1.

## Final Conclusion
Audited 11 workspace crates, tests (36 E2E cases), and bench (9 cases). Recorded 14 crate-level liabilities with evidence. E2E suite includes 13 skipped cases. Bench suite includes 7 regression-gate and 2 exploratory cases.

## TODO
- Remove request-path panics and snapshot fallback in `crates/pavis`.
- Replace panic-on-lock/enum paths in `crates/pavis-core`, `crates/pavis-codec-serde`, and `crates/pavis-testkit` with explicit errors.
- Add size guards for unbounded reads in `crates/pavis-ingest-file` and `crates/pavis-relay`.
- Document unsafe assumptions and add guardrail tests for request ID UTF-8 and config validation paths.
- Reduce skipped E2E cases and stabilize timing-sensitive tests.

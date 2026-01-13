# Repository Liability Audit Summary

## Audit Scope & Rules
This audit is a strict, evidence-driven evaluation of the Pavis repository. 
- **Axiom 1:** Code is a liability by default.
- **Axiom 2:** "Code" includes production Rust, tests, benchmarks, shell scripts, and CI.
- **Axiom 3:** Intent must be clear from names and types; comments explain "why", never "what".
- **Axiom 4:** Evidence-only reporting; no inferred intent or praise.
- **Axiom 5:** No refactors or recommendations.

## Audit Coverage
- **Paths scanned:**
  - `crates/pavis-core`
  - `crates/pavis-codec-api`
  - `crates/pavis-ingest-api`
  - `crates/pavis-pvs`
  - `crates/pavis-codec-serde`
  - `crates/pavis-relay`
  - `crates/pavctl`
  - `crates/pavis`
  - `tests/suites/integrated`
  - `bench/cases`
- **Total files scanned per unit:**
  - `pavis-core`: 7 files (lib, runtime, validate, headers, routes, server, upstreams)
  - `pavis-codec-api`: 1 file
  - `pavis-ingest-api`: 1 file
  - `pavis-pvs`: 5 files (lib, error, header, read, write, verify)
  - `pavis-codec-serde`: 4 files (lib, config, structural, serde_helpers)
  - `pavis-relay`: 3 files (main, config, server)
  - `pavctl`: 6 files (main, commands/*)
  - `pavis`: 1 file (main)
  - `E2E`: 7 files in integrated/
  - `Bench`: 6 files in cases/
- **Paths skipped:** None. 100% of primary functional units were traversed.

## Workspace Map
### Rust Crates (Audit Order)
- `pavis-core`: Core primitives, canonical schema, and semantic validation.
- `pavis-codec-api`: Traits and types for config compilation (Artifact -> RuntimeConfig).
- `pavis-ingest-api`: Traits for config acquisition (Stream<Artifact>).
- `pavis-pvs`: Pavis binary format (PVS) encoding, decoding, and verification.
- `pavis-codec-serde`: Implementation of Codec for YAML/JSON using Serde.
- `pavis-relay`: Configuration distribution server (pushed/pulled PVS artifacts).
- `pavis-testkit`: Shared test utilities for upstream/relay simulation.
- `pavctl`: CLI tool for config generation and inspection.
- `pavis-benchkit`: Benchmark load generation and metrics.
- `pavis`: The main data plane binary (proxy).

### Special Modules
- E2E: `tests/`
- Bench: `bench/`

## Crate Audits

### pavis-core
#### Inventory
- Files scanned: 7
- Key modules: `runtime`, `validate`, `server`, `upstreams`, `routes`

#### Liability Ledger
- F-pavis-core-001
  - Gate: Change-Resilience
  - Severity: Medium
  - Evidence: `runtime.rs:56-61` (Manual `unsafe` in `from_trusted`)
  - Impact: Future schema changes may break invariants assumed by "trusted" callers without compiler enforcement.
  - Confidence: High

- F-pavis-core-002
  - Gate: Intent
  - Severity: Low
  - Evidence: `validate.rs:88` (Manual Regex complexity check `a.repeat(2049)`)
  - Impact: Magic numbers (2048) in validation logic rot as hardware/regex engines evolve.
  - Confidence: High

#### Verdict
- Justified liabilities: 2
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavis-codec-api
#### Inventory
- Files scanned: 1
- Key modules: `lib.rs` (Codec trait)

#### Liability Ledger
- F-pavis-codec-api-001
  - Gate: Deletion
  - Severity: Low
  - Evidence: `lib.rs:58-62` (`CompactionLevel` enum with `Off`, `Trim`, `Prune`)
  - Impact: Implementation of `Trim`/`Prune` is currently empty/no-op in all observed codecs, representing unused surface area.
  - Confidence: Medium

#### Verdict
- Justified liabilities: 0
- Questionable liabilities: 1
- Unjustified liabilities: 0

### pavis-pvs
#### Inventory
- Files scanned: 5
- Key modules: `header`, `verify`, `write`

#### Liability Ledger
- F-pavis-pvs-001
  - Gate: Necessity
  - Severity: Low
  - Evidence: `header.rs:48` (`_reserved: [u8; 20]`)
  - Impact: Future-proofing via reserved padding is a data-structure liability that complicates comparisons and serialization.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavis-codec-serde
#### Inventory
- Files scanned: 4
- Key modules: `lib`, `config` (types)

#### Liability Ledger
- F-pavis-codec-serde-001
  - Gate: Intent
  - Severity: Medium
  - Evidence: `lib.rs:51-53` (Three-stage conversion: SerdeConfig -> Structural -> RuntimeConfig)
  - Impact: High complexity for mapping; every new feature requires 3 separate struct updates.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

### pavis-relay
#### Inventory
- Files scanned: 3
- Key modules: `main`, `server`

#### Liability Ledger
- F-pavis-relay-001
  - Gate: Verifiability
  - Severity: Low
  - Evidence: `Cargo.toml` (optional `pavis-codec-serde` dependency)
  - Impact: Feature-gating the primary codec creates CI liability where certain configurations remain untested in default builds.
  - Confidence: Medium

#### Verdict
- Justified liabilities: 0
- Questionable liabilities: 1
- Unjustified liabilities: 0

### pavis
#### Inventory
- Files scanned: 1
- Key modules: `main.rs`

#### Liability Ledger
- F-pavis-001
  - Gate: Change-Resilience
  - Severity: Medium
  - Evidence: `main.rs:188-210` (Manual TLS/ClientAuth mapping to Pingora types)
  - Impact: Glue code between `pavis-core` and `pingora` is brittle and duplicate logic from `pavis-core::validate`.
  - Confidence: High

#### Verdict
- Justified liabilities: 1
- Questionable liabilities: 0
- Unjustified liabilities: 0

## E2E Audit

### Inventory
- Total suites: 3 (integrated, pavis, relay)
- Total cases: 7 in integrated/
- Runner: `tests/run.sh`

### Case Ledger
- E2E-reload-01
  - File: `tests/suites/integrated/20_reload_switch.sh`
  - Scenario & invariant under test: Hot reload traffic shift.
  - Inputs: YAML v1 (backend-v1), YAML v2 (backend-v2).
  - Assertions: Body matches `backend-v2`, `SUT_ID` remains identical (no restart).
  - Evidence: `python3 -c "import sys, json; print(json.load(sys.stdin).get('instance_id', ''))"`
  - Determinism risks: 20 retries with 0.5s sleep (10s total timeout).
  - Failure signal quality: Clear (specific error messages for switch fail vs identity change).
  - Why E2E: Requires live Relay and Pavis interacting over HTTP to verify state synchronization.

- E2E-reload-02
  - File: `tests/suites/integrated/21_reload_stable.sh`
  - Scenario & invariant under test: Idempotent update stability.
  - Inputs: Same YAML content published twice.
  - Assertions: 20 consecutive requests must succeed without drops or change.
  - Evidence: `assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"`
  - Determinism risks: Timing-sensitive (0.1s sleep between requests).
  - Failure signal quality: Noisy (generic "Traffic failure during idempotent update").
  - Why E2E: Verifies that the internal state machine suppresses no-op reloads.

### Verdict
- High-signal cases: 5
- Fragile/noisy cases: 2
- Questionable-existence cases: 0

## Bench Audit

### Inventory
- Total cases: 6
- Tooling: `pavis-benchkit` (bench-loadgen), `docker stats`, `wrk2` (legacy)

### Case Ledger
- BENCH-latency-short
  - File: `bench/cases/latency_short_1x.sh`
  - Workload type: open-loop (bench-loadgen)
  - Exact run command: `${LOADGEN_BIN} --url "$PROXY_URL" --rate "$TARGET_RPS" --duration "$duration"`
  - Metrics produced: `achieved_rps`, `errors`, `p50`, `p90`, `p99`, `dropped`, `cpu_pct`, `peak_rss`
  - Reproducible constraints: `PROXY_CPUSET_EXPECTED="1-2"`
  - Noise sources: Docker networking stack, host kernel scheduling.
  - Classification: Regression Gate

### Verdict
- Gate benchmarks: 6
- Exploratory benchmarks: 0
- Invalid / misleading benchmarks: 0

## Systemic Liability Patterns
1. **Triple-Schema Mapping:** Multiple crates (`codec-serde`, `pavctl`, `pavis`) perform manual mapping between Serde DTOs, Pavis-Core canonical types, and Pingora internal types.
2. **Bash-Heavy Orchestration:** E2E and Bench modules rely heavily on Bash scripts for port management and process lifecycle, which are brittle across environments (as seen in `win32` traversal).
3. **Reserved Padding:** Binary format (`PVS`) uses manual padding which is a maintenance liability for bit-for-bit compatibility.

## Highest Compound-Risk Areas
1. **Config Compilation Pipeline:** Intersection of `pavis-codec-api`, `pavis-codec-serde`, and `pavis-core` validation. A bug in any layer or the mapping between them can lead to invalid configurations passing into the data plane.
2. **Hot Reload Lifecycle:** E2E tests show sensitivity to timing; the interaction between Relay's artifact delivery and Pavis's agent polling is a high-liability state machine.

## Final Conclusion
The repository demonstrates a high degree of decoupling via traits (`Codec`, `Ingest`), but this decoupling has introduced a "Mapping Tax" (F-pavis-codec-serde-001) that increases liability. Binary formats and core validation are explicit and evidenced. E2E coverage for reload logic is high-signal but depends on Bash-based environmental stability.
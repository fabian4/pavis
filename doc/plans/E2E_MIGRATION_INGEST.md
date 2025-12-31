# Plan: E2E Migration to File Ingestion & Structural Refactoring

**Status**: Draft
**Target**: `crates/pavis-e2e` (Relay & Integrated Suites)

## 1. Objective
Migrate all Relay and Integrated E2E test cases to use the `pavis-ingest-file` pipeline instead of manual `pavctl` generation or direct REST API publishing. Simultaneously refactor the test structure to ensure strict isolation (one test per file) and clear input/output abstractions.

## 2. Core Principles
- **Ingestion Realism**: Use the file-watcher pipeline as the primary way to push configurations to the Relay.
- **Strict Isolation**: One Rust file per `#[tokio::test]`.
- **Declarative Testing**: Abstract boilerplate (spawning processes, waiting for health) into shared modules with clear `input` (YAML/Config) and `output` (Assertions).
- **Zero pavctl**: Remove dependency on the `pavctl` binary for Integrated and Relay E2E suites.

## 3. Abstraction Layer (`pavis-e2e/src/support/`)

### 3.1 Scenario Builder
Introduce a `PavisScenario` fixture that manages the lifecycle of Relay, Pavis, and Upstreams.

```rust
pub struct PavisScenario {
    pub relay: RelayEnv,
    pub pavis: Option<PavisEnv>,
    pub upstreams: UpstreamSet,
}

impl PavisScenario {
    /// Applies a configuration by writing YAML to the Relay's ingest directory.
    pub async fn apply_config(&self, config: &RuntimeConfig) -> Result<()> { ... }
    
    /// High-level assertions.
    pub async fn expect_body(&self, expected: &str) -> Result<()> { ... }
    pub async fn expect_relay_version(&self, version: u64) -> Result<()> { ... }
}
```

### 3.2 Configuration Templates
Provide shared templates in `support/configs.rs` to generate standard `RuntimeConfig` objects, which are then serialized to YAML by the fixture.

## 4. Proposed File Structure

### 4.1 Relay Suite (`tests/relay/`)
Refactor `relay.rs` into:
- `tests/relay/mod.rs` (Module declarations and sequence documentation)
- `tests/relay/publish_success.rs`
- `tests/relay/invalid_yaml_ingest.rs`
- `tests/relay/long_poll_timeout.rs`
- `tests/relay/debounce_logic.rs`
- `tests/relay/persistence_recovery.rs`

### 4.2 Integrated Suite (`tests/integrated/`)
Refactor `integrated/mod.rs` and submodules:
- `tests/integrated/mod.rs`
- `tests/integrated/pipeline_basic.rs`
- `tests/integrated/recovery_after_relay_crash.rs`
- `tests/integrated/concurrency_limits.rs`
- `tests/integrated/observability_metrics.rs`

## 5. Implementation Roadmap

### Phase 1: Support Refactoring
1.  **YAML Serialization**: Add `pavis_codec_serde` based helpers to convert `RuntimeConfig` to YAML.
2.  **RelayEnv Update**: Enable `file_ingest` by default in `RelayOptions`.
3.  **Fixture Creation**: Implement `PavisScenario` to wrap `RelayEnv` and `PavisEnv`.

### Phase 2: Relay Suite Migration
1.  Split `relay.rs` into individual files in `tests/relay/`.
2.  Convert `/v1/publish` calls to `scenario.apply_config()`.
3.  Standardize assertions using the new support helpers.

### Phase 3: Integrated Suite Migration
1.  Refactor `tests/integrated/` submodules to move tests into the root of the suite or a flattened structure.
2.  Replace all `pavctl gen` and manual `pvs` file management with the `pavis-ingest-file` flow.
3.  Update `PavisEnv` to consume configs exclusively via the Relay URL.

### Phase 4: Cleanup
1.  Remove `generate_pvs` from `pavis-e2e/src/support/pavis/process.rs`.
2.  Update `Cargo.toml` to remove unnecessary dependencies if applicable.
3.  Update CI/scripts (`e2e-integrated.sh`, `e2e-relay.sh`) to stop building `pavctl`.

## 6. Definition of Done
- All Integrated and Relay E2E tests pass using file ingestion.
- `pavctl` is not used in any code path within `tests/integrated/` or `tests/relay/`.
- Each test resides in its own `.rs` file.
- Code duplication in tests is reduced through the use of `PavisScenario`.

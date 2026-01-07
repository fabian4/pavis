# Audit Phase 1: Boundary & Responsibility Audit
- Target: `crates/pavis`
- Timestamp: 2026-01-07T00:00:00Z
- AI Model: gemini-2.0-flash-exp

## 1. Input Boundary Verification

The runtime adheres to strict input boundaries.

*   **Config Loading**: The `load::load_file` function (and `agent::lkg::load_lkg_config`) delegates entirely to `pavis_pvs::load` for deserializing the `.pvs` binary artifact. It then wraps the result in `ValidatedRuntimeConfig` via `assume_validated`.
*   **Raw Parsing Absence**: There is no code in `crates/pavis` that attempts to parse YAML, JSON, or TOML configuration files. The `serde` dependency is not present in `Cargo.toml` (except transitively), and `pavis-codec-serde` is not linked.
*   **File I/O**: File system access is restricted to:
    *   Loading the `.pvs` binary artifact.
    *   Reading the `.pvs.version` metadata file (for LKG management).
    *   Writing access logs (if configured).
    *   Loading TLS certificates (via Pingora).

**Verdict**: **PASS**. The runtime consumes only validated artifacts and does not perform raw config parsing.

## 2. Responsibility Audit

The runtime largely avoids policy inference, with one minor execution-time safe default.

*   **Validation**: The runtime trusts the `ValidatedRuntimeConfig` and performs no semantic validation of its own. It assumes `pavis-core` and `pavctl` have done this work.
*   **Defaults**:
    *   **Configuration Defaults**: No evidence found of the runtime populating missing configuration fields (e.g., timeouts, buffer sizes) with default values. It uses the values provided in the struct.
    *   **Execution Defaults**:
        *   **SNI**: In `src/proxy/service.rs`, if TLS is enabled but no SNI is configured or rewritten, the runtime defaults to `localhost`:
            ```rust
            let sni_value = sni
                .or_else(|| ctx.sni_override.clone())
                .unwrap_or_else(|| Hostname("localhost".to_string()));
            ```
            *Assessment*: This is acceptable as a "safety net" for the underlying TLS handshake client, preventing runtime errors, rather than a policy inference that changes the system's intended behavior.

**Verdict**: **PASS**. The separation of concerns is respected.

## 3. Dependency Audit

`crates/pavis/Cargo.toml` was inspected.

*   **Permitted**:
    *   `pingora`: Core proxy engine.
    *   `pavis-core`, `pavis-pvs`: Internal dependencies for types and loading.
    *   `tokio`, `tracing`, `clap`: Standard runtime infrastructure.
*   **Forbidden (and Absent)**:
    *   `serde_yaml`, `serde_json`, `toml`: No config parsers.
    *   `pavis-codec-*`: No codec logic.
    *   `pavis-relay`: No control plane logic.

**Verdict**: **PASS**. The dependency tree is clean and aligned with the runtime-only scope.
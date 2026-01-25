# Dependency Upgrade Plan (2026-01-25T09:58:28.625Z)

## Executive Summary
Execute a single “big-bang” PR that upgrades every workspace dependency—unifying the HTTP/axum/tower stack, bumping observability crates, and closing major-version gaps (thiserror, notify, ureq, webpki-roots)—while sequencing the work into tightly validated checkpoints. The biggest risks are HTTP body/type mismatches once hyper 1.x + http 1.x land, and silent regressions in metrics/OTel exporters; we mitigate by upgrading stack-by-stack with interim `cargo check`/targeted test runs and by verifying dependency deduplication (`cargo tree -d`).

## Step-by-Step Execution Plan

1. **Workspace Prereq Sweep**  
   - *Changes*: In `Cargo.toml` `[workspace.dependencies]`, bump baseline crates to latest stable versions that all downstream crates will use (tokio 1.49, anyhow 1.0.100, serde 1.0.228, tracing 0.1.44, etc.). Ensure shared versions are declared only once; remove stale patch overrides.  
   - *Expected Failure*: Minimal; possible feature mismatches if local crate-specific features relied on older defaults.  
   - *Fix Strategy*: If a crate needed extra features, enable them explicitly in its local `Cargo.toml`.  
   - *Validation*: `cargo check --workspace` (Validation #1).  
   - *Checkpoint*: All workspace crates compile with old HTTP stack still in place.  
   - *Status*: ✅ Completed 2026-01-25T13:47:54Z (`cargo check --workspace`, `make ci-local`).

2. **HTTP Core Unification (http/tokio-util/http-body)**  
   - *Changes*: Update every `Cargo.toml` referencing `http = "0.2"` to `http = "1.4"`; bump `http-body`/`http-body-util` to 1.x where used. Replace `hyper = "0.14"` and legacy `hyper-util` deps with `hyper = "1.8"` and `hyper-util = "0.1"` (runtime, relay, benchkit). Update `reqwest` to 0.13.x (still hyper 1 compatible).  
   - *Expected Failure*: Compile errors around `Request`/`Response` APIs (header mutability, method constructors), `Body` trait changes, and `hyper::client::Builder` signatures.  
   - *Fix Strategy*: Refactor call sites to use `http::request::Builder::new()` semantics, swap `hyper::Body` usages to `hyper::body::Bytes`/`Incoming`, and adjust `Service` impls to new generics.  
   - *Validation*: `cargo check -p pavis` and `cargo check -p pavis-benchkit` (Validation #2).  
   - *Checkpoint*: Both runtime and benchkit compile with the unified HTTP stack.  
   - *Status*: ✅ Completed 2026-01-25T14:30:50Z (`cargo check -p pavis-benchkit`, `cargo check -p pavis`, `make ci-local`).

3. **Axum/Tower/Tower-HTTP Alignment**  
   - *Changes*: In runtime/relay/testkit `Cargo.toml`, set `axum = "0.8"`, `axum-server = "0.8"`, `tower = "0.5"`, `tower-http = "0.6"`. Ensure `tower` 0.4 no longer appears; update features (e.g., `tower = { version = "0.5", features = ["util", "timeout"] }`).  
   - *Expected Failure*: Handler signatures now require `State` extractor updates, `TypedHeader` import paths changed, `tower::ServiceBuilder` moved modules.  
   - *Fix Strategy*: Update handlers to new extractor APIs, replace `Router::route_layer` patterns with `middleware::from_fn`, adjust testkit mocks to new `tower::Service` trait.  
   - *Validation*: `cargo test -p pavis-testkit` (Validation #3).  
   - *Checkpoint*: All axum/tower users compile/tests pass; `cargo tree -d | grep -E "http|tower"` shows single major per crate.

4. **Foundation Upgrade: thiserror 2.x**  
   - *Changes*: Set `thiserror = "2"` in every crate that depends on it (codec, core, ingest, relay, runtime).  
   - *Expected Failure*: Macro expansion errors requiring `#[derive(Debug)]`, `#[error(transparent)]` semantics, or missing `source` fields.  
   - *Fix Strategy*: Ensure each error enum derives `Debug`, update `#[error(transparent)]` variants to include `#[from]` as needed, add `#[error("...")]` strings where previously derived automatically.  
   - *Validation*: `cargo check -p pavis-core && cargo check -p pavis-relay` (Validation #4).  
   - *Checkpoint*: Core crates compile with new macros; no lingering `thiserror` 1.x in `cargo tree -d`.

5. **Observability Stack Refresh (metrics + exporter)**  
   - *Changes*: In runtime `Cargo.toml`, bump `metrics = "0.24"`, `metrics-exporter-prometheus = "0.18"`, `metrics-util = "0.16"` (if present). Update initialization code to new builder API (e.g., `PrometheusBuilder::new()...install_recorder()`).  
   - *Expected Failure*: Recorder builder signature differences, label API rename (`Recorder::describe_counter` -> instrumentation macros).  
   - *Fix Strategy*: Follow upstream migration guides; encapsulate metrics setup in helper returning `RecorderHandle`.  
   - *Validation*: Run `cargo test -p pavis --features metrics && tests/suites/pavis/80_observability_metrics_contract.sh` (Validation #5).  
   - *Checkpoint*: Metrics e2e passes; runtime logs show successful exporter startup.

6. **Observability Stack Refresh (OpenTelemetry + tracing)**  
   - *Changes*: Upgrade `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, and `tracing-opentelemetry` to 0.31.x; adjust feature flags (e.g., `rt-tokio`, `metrics`).  
   - *Expected Failure*: Build errors due to renamed builders (`SdkTracerProvider::builder()` API), resource detection changes, `TracerProvider` type mismatches.  
   - *Fix Strategy*: Follow 0.31 upgrade notes—use `SdkTracerProvider::builder().with_batch_exporter(...)`, update OTLP exporter config to builder-based pattern, replace deprecated `global::set_tracer_provider`.  
   - *Validation*: `cargo test -p pavis --lib telemetry` plus targeted e2e `tests/suites/pavis/82_observability_tracing_context.sh`.  
   - *Checkpoint*: Tracing spans still emitted (verify via logs or mock collector).

7. **File Watcher Modernization (notify 8.x)**  
   - *Changes*: In `crates/pavis-ingest-file/Cargo.toml`, bump `notify = "8"`. Refactor watcher initialization to use `RecommendedWatcher::new(move |res| ...)` async-compatible API.  
   - *Expected Failure*: Compile errors around old `Watcher::new_immediate`, runtime panics due to channel lifetimes.  
   - *Fix Strategy*: Wrap watcher in tokio task, use `notify::Config::default().with_poll_interval(...)`, ensure senders are `async_channel` or tokio mpsc.  
   - *Validation*: `cargo test -p pavis-ingest-file` and run ingest-related e2e case (pick test referencing file ingest).  
   - *Checkpoint*: Ingest tests pass; logs confirm file change detection.

8. **TLS / Client Stack Updates (ureq + webpki-roots)**  
   - *Changes*: In `crates/pavctl/Cargo.toml`, set `ureq = "3"`; update runtime `Cargo.toml` to `webpki-roots = "1"`. Ensure `reqwest` (already 0.13) uses `rustls-tls` features matching new roots.  
   - *Expected Failure*: TLS handshake failures due to changed default roots; compile errors for removed builder APIs in ureq (e.g., `.call()` semantics).  
   - *Fix Strategy*: Replace deprecated `AgentBuilder` methods, explicitly load custom certs if required. For runtime, verify `rustls::RootCertStore` uses `webpki_roots::TLS_SERVER_ROOTS`.  
   - *Validation*: `cargo test -p pavctl` and run TLS suites `tests/suites/pavis/70_security_tls.sh` & `71_security_inbound_mtls.sh`.  
   - *Checkpoint*: CLI publish works against relay; TLS e2e green.

9. **Serde YAML Tech-Debt Check**  
   - *Changes*: If `serde_yaml 0.9.34+deprecated` breaks due to other upgrades, switch to maintained fork (e.g., `serde_yml`) or pin to latest patch with note in README; otherwise leave pinned but add TODO comment in `Cargo.toml`.  
   - *Expected Failure*: Build warnings only; potential runtime parse differences.  
   - *Fix Strategy*: If migrating, adjust YAML loader code to new API (usually identical).  
   - *Validation*: `cargo test -p pavctl -- lib` (config generation tests).  
   - *Checkpoint*: YAML parsing tests pass; plan documents remaining tech debt.

10. **Workspace Cleanup & Lock Refresh**  
    - *Changes*: Run `cargo update`, inspect `Cargo.lock` for duplicate majors (esp. http/hyper/tower). Remove unused dependency entries/features surfaced during refactors; run `cargo fmt` (if already enforced).  
    - *Expected Failure*: Build may fail if features disabled inadvertently.  
    - *Fix Strategy*: Reintroduce required features; keep commit history linear.  
    - *Validation*: `cargo tree -d | grep -E "http|hyper|tower"` (should show single versions), `cargo tree -d | grep notify` (single 8.x), `cargo check --workspace` (Validation #6).  
    - *Checkpoint*: Dependency graph deduped; workspace builds cleanly.

11. **Final CI Pass**  
    - *Changes*: None—execute full CI pipeline.  
    - *Expected Failure*: Integration/e2e regressions (TLS, observability, reload).  
    - *Fix Strategy*: Triage by subsystem—if TLS fails, re-check cert roots; if observability fails, inspect exporter config; if routing fails, revalidate axum handler changes.  
    - *Validation*: `make ci-local` (Validation #7).  
    - *Checkpoint*: All CI stages green; ready for PR review.

## Definition of Done
- `cargo tree -d | grep -E "http|hyper|tower"` shows only `http 1.x`, `hyper 1.x`, `tower 0.5`, `tower-http 0.6`.  
- `cargo tree -d | grep -E "notify|ureq|webpki-roots|thiserror"` confirms single major versions (notify 8, ureq 3, webpki-roots 1, thiserror 2).  
- All targeted validations (7 checkpoints above) executed successfully.  
- `make ci-local` passes; if it fails, inspect (1) HTTP/tower compile errors, (2) TLS/observability e2e logs, (3) ingest watcher behavior, in that order.  
- Lockfile committed with no unused dependencies; documentation updated for any remaining serde_yaml tech debt.

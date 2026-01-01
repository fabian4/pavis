## 📌 Overall Summary (Latest)

🚫 Blocker: 0 · 🔥 High: 0 · ⚠️ Medium: 0 · 🧹 Low: 0 · ✅ Resolved: 8

---

## Open Findings (Prioritized)

No open findings.

---

## Review Entry — 2026-01-01T03:11:42Z

### Scope
- Public API surface scan across all crates.

---

### Method
- Analysis of `pub` exports, visibility modifiers, and boundary types.

### Model
- claude-sonnet-4-20250514

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| — | — | All crates | Public API boundaries healthy | Done |

---

### Detailed Findings

#### Public API Analysis

**pavis-core (Library API — intentional public surface):**
- ✅ `RuntimeConfig`, `ValidatedRuntimeConfig`: Core domain types
- ✅ `ServerConfig`, `TlsConfig`: Server configuration
- ✅ `VirtualHost`, `Route`, `MatchType`, `WeightedDestination`: Routing
- ✅ `Upstream`, `Endpoint`, `LoadBalancer`, `HttpVersion`: Upstream
- ✅ `TelemetryConfig`, `AccessLogConfig`, `LogLevel`: Telemetry
- ✅ `validate_runtime`, `CoreValidationError`: Validation entry point
- ✅ `ValidatedRuntimeConfig::from_trusted`: Properly `unsafe` with safety docs

**pavis-pvs (Library API — integrity boundary):**
- ✅ `PvsHeader`, `PvsHeaderView`, `VerifiedPvs`: Header types
- ✅ `load`, `verify`, `inspect`, `encode`, `write`: Operations
- ✅ `PvsError`, `PvsResult`: Error types
- ✅ Constants: `PAVIS_MAGIC`, `PAVIS_VERSION`, header names

**pavis-relay (Binary — minimal lib surface):**
- ✅ `serve_from_config`: Single public entry point
- ✅ `config` module: Config types for YAML parsing
- ✅ Internal state/handlers correctly `pub(crate)`

**pavis (Binary — minimal lib surface):**
- ✅ Module re-exports for internal use
- ✅ No unintended public API leakage

**Codec/Ingest APIs:**
- ✅ `pavis-codec-api`: `Codec` trait, `CheckedArtifact`, `CodecError`
- ✅ `pavis-ingest-api`: `Artifact`, `SourceInfo`, `IngestError`
- ✅ `pavis-codec-serde`: `SerdeCodec`, `PavisConfig`

No public API boundary violations found.

---

## Review Entry — 2025-12-30T12:57:53Z

### Scope
- Public API surface scan across all crates (targeted fix verification).
- Snapshot: `e434c0ba4c9250b166cb8a53c63cb13ef96bd892`

---

### Method
- Manual verification that relay state types are no longer exposed via the public lib API.


### Model
- GPT-5

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | Low | Relay API | Relay re-exports internal state types | Done |

---

### Detailed Findings

#### F-1: Relay re-exports internal state types
- **Expectation:** Binary-oriented crates avoid exposing internal state types unless required by external consumers.
- **Observed:** Relay state and options are now crate-private; public API exposes only `serve_from_config` and config types.
- **Evidence:** `crates/pavis-relay/src/lib.rs`; `crates/pavis-relay/src/state.rs`; `crates/pavis-relay/src/app.rs`.
- **Assessment (Reason):** Public surface reduced while keeping binary entrypoint intact.
- **Recommendation (Suggestion):** None.
- **Doc Drift?:** No.

---

## Review Entry — 2025-12-30T11:35:29Z

### Scope
- Public API surface scan across all crates.

---

### Method
- Manual scan of `pub` exports in library crates and their external usage.


### Model
- GPT-5

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | Low | Relay API | Relay re-exports internal state types | Open |

---

### Detailed Findings

#### F-1: Relay re-exports internal state types
- **Expectation:** Binary-oriented crates avoid exposing internal state types unless required by external consumers.
- **Observed:** `pavis-relay` re-exports `RelayState`, `RelayOptions`, `RelayError`, and `execute_plan` from its library API; only integration tests appear to rely on these types.
- **Evidence:** `crates/pavis-relay/src/lib.rs`; `crates/pavis-relay/tests/relay_http.rs`.
- **Assessment (Reason):** Expands the public API surface and creates stability obligations for internal types.
- **Recommendation (Suggestion):** Consider moving tests into `src/` modules and reducing exports to `pub(crate)` where external use is not required.
- **Doc Drift?:** No.

---

## Review Entry — 2025-12-30T04:45:12Z

### Scope
- Public API surface scan across all crates.

---

### Method
- Targeted scan of `pub` types and boundary bypass methods.


### Model
- gemini-2.0-flash-exp

---

### Summary (Index)

No new findings. Public API boundaries remain stable. The `unsafe` marker on `ValidatedRuntimeConfig::from_trusted` is correctly enforcing explicit opt-in for validation bypass.

---

> Older review entries continue below this point, in reverse chronological order.

## Review Entry — 2025-12-30T03:33:22Z

### Scope
- `crates/pavis-core` and `crates/pavis` public API surface.

---

### Method
- Targeted scan of public constructors and validation bypass paths.


### Model
- GPT-5

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | High | Core validation | Validation bypass made explicit and unsafe | Done |

---

### Detailed Findings

#### F-1: Validation bypass made explicit and unsafe
- **Expectation:** Validation bypass must be tightly controlled and explicitly marked.
- **Observed:** `ValidatedRuntimeConfig::from_trusted` is `pub unsafe fn` with safety docs; call sites require `unsafe` blocks.
- **Evidence:** `crates/pavis-core/src/runtime.rs` exposes `pub unsafe fn from_trusted`; `crates/pavis/src/load.rs` uses an `unsafe` block.
- **Assessment (Reason):** Makes boundary bypass explicit and opt-in for trusted callers.
- **Recommendation (Suggestion):** Keep unsafe API documentation explicit and limit usage to trusted contexts.
- **Doc Drift?:** No.

---

## Review Entry — 2025-12-30T03:25:01Z

### Scope
- `crates/pavis-core` public API surface.

---

### Method
- Targeted scan of public constructors for validation bypass risk.


### Model
- GPT-5

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | High | Core validation | `ValidatedRuntimeConfig::from_trusted` bypass is public | In Progress |

---

### Detailed Findings

#### F-1: `ValidatedRuntimeConfig::from_trusted` bypass is public
- **Expectation:** Validation bypass is not publicly exposed without explicit safeguards.
- **Observed:** `ValidatedRuntimeConfig::from_trusted` is `pub fn` with no unsafe marker.
- **Evidence:** `crates/pavis-core/src/runtime.rs` (`pub fn from_trusted`).
- **Assessment (Reason):** External crates can bypass canonical validation, undermining boundary guarantees.
- **Recommendation (Suggestion):** Restrict to `pub(crate)` or mark `unsafe` with clear safety docs.
- **Doc Drift?:** No.

---

## Review Entry — 2025-12-30T03:18:52Z

### Scope
- `crates/pavis-core` and `crates/pavis` public API surface.

---

### Method
- Targeted review of routing model exposure in core.


### Model
- GPT-5

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | Medium | Core routing model | Runtime-only regex removed from core API | Done |

---

### Detailed Findings

#### F-1: Runtime-only regex removed from core API
- **Expectation:** Core public models avoid runtime-only state.
- **Observed:** `Route` no longer includes `compiled_regex` in the core public API.
- **Evidence:** `crates/pavis-core/src/runtime/routing.rs` no longer includes `compiled_regex`.
- **Assessment (Reason):** Removes runtime-only type exposure and reduces coupling.
- **Recommendation (Suggestion):** None.
- **Doc Drift?:** No.

---

## Review Entry — 2025-12-29T17:42:57Z

### Scope
- Public API surface scan across all crates.

---

### Method
- Manual scan of `pub` types and constructors for boundary leakage.


### Model
- GPT-5

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | High | Core validation | `ValidatedRuntimeConfig::from_trusted` bypass is public | Open |
| F-2 | Medium | Core routing model | Runtime-only regex exposed in core API | Open |

---

### Detailed Findings

#### F-1: `ValidatedRuntimeConfig::from_trusted` bypass is public
- **Expectation:** Validation bypass is not publicly exposed without explicit safeguards.
- **Observed:** Public constructor allows unchecked `RuntimeConfig` without validation.
- **Evidence:** `crates/pavis-core/src/runtime.rs` (`pub fn from_trusted`).
- **Assessment (Reason):** External callers can bypass canonical validation, weakening boundary guarantees.
- **Recommendation (Suggestion):** Restrict to `pub(crate)` or mark `unsafe` with explicit safety docs.
- **Doc Drift?:** No.

#### F-2: Runtime-only regex exposed in core API
- **Expectation:** Core public types avoid runtime-only implementation details.
- **Observed:** `Route` includes `compiled_regex: Option<regex::Regex>`.
- **Evidence:** `crates/pavis-core/src/runtime/routing.rs`.
- **Assessment (Reason):** Couples core API to runtime-only details and externalizes `regex` dependency.
- **Recommendation (Suggestion):** Move compiled regex to runtime wrappers or make field private with accessors.
- **Doc Drift?:** No.

---

## Review Entry — 2025-12-29T12:52:30Z

### Scope
- Report format alignment only.

---

### Method
- Report structure update to match the standardized template.


### Model
- GPT-5

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | Nit | Report | No public API findings in this pass | Done |

---

### Detailed Findings

#### F-1: No public API findings in this pass
- **Expectation:** Report updates should not introduce new findings without evidence.
- **Observed:** No new public API findings recorded in this pass.
- **Evidence:** `audit/report/PUBLIC_API_REVIEW.md` update only.
- **Assessment (Reason):** No changes to public API status.
- **Recommendation (Suggestion):** None.
- **Doc Drift?:** No.

---

## Review Entry — 2025-12-29T12:47:09Z

### Scope
- Public API boundary fixes in `pavis` and `pavis-pvs`.

---

### Method
- Verification of visibility changes against prior findings.


### Model
- GPT-5

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | Medium | PVS API surface | Semantic validation removed from `pavis-pvs` API | Done |
| F-2 | Medium | Runtime routing | Internal routing representation no longer public | Done |
| F-3 | Low | Runtime load balancing | Internal counter type no longer public | Done |
| F-4 | Low | Runtime context | Request context type visibility reduced | Done |

---

### Detailed Findings

#### F-1: Semantic validation removed from `pavis-pvs` API
- **Expectation:** `pavis-pvs` exposes integrity validation only.
- **Observed:** Public `load_validated` removed from `pavis-pvs` API surface.
- **Evidence:** `crates/pavis-pvs/src/lib.rs` no longer re-exports `load_validated`.
- **Assessment (Reason):** Restores integrity-only boundary for PVS.
- **Recommendation (Suggestion):** None.
- **Doc Drift?:** No.

#### F-2: Internal routing representation no longer public
- **Expectation:** Runtime internal routing details are crate-private.
- **Observed:** `CompiledVirtualHost` visibility reduced.
- **Evidence:** `crates/pavis/src/router.rs` visibility reduced for `CompiledVirtualHost`.
- **Assessment (Reason):** Reduces coupling to internal routing types.
- **Recommendation (Suggestion):** None.
- **Doc Drift?:** No.

#### F-3: Internal counter type no longer public
- **Expectation:** Internal load-balancing state remains private.
- **Observed:** `AlignedCounter` visibility reduced.
- **Evidence:** `crates/pavis/src/upstream/cluster.rs` visibility reduced for `AlignedCounter`.
- **Assessment (Reason):** Limits public API to stable types.
- **Recommendation (Suggestion):** None.
- **Doc Drift?:** No.

#### F-4: Request context type visibility reduced
- **Expectation:** Request context types are not exposed unless part of stable public API.
- **Observed:** `RouterContext` visibility reduced and re-export removed.
- **Evidence:** `crates/pavis/src/proxy/context.rs` and `crates/pavis/src/proxy.rs`.
- **Assessment (Reason):** Removes internal runtime context from public API surface.
- **Recommendation (Suggestion):** None.
- **Doc Drift?:** No.

---

## Review Entry — 2025-12-29T12:40:26Z

### Scope
- Public API surface scan across all crates.

---

### Method
- Manual scan of `pub` types and re-exports for boundary leakage.


### Model
- GPT-5

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | Medium | PVS API surface | Semantic validation exposed in `pavis-pvs` | Open |
| F-2 | Medium | Runtime routing | Internal routing representation public | Open |
| F-3 | Low | Runtime load balancing | Internal counter type public | Open |
| F-4 | Low | Runtime context | Request context type public | Open |

---

### Detailed Findings

#### F-1: Semantic validation exposed in `pavis-pvs`
- **Expectation:** `pavis-pvs` exposes integrity validation only.
- **Observed:** Public API includes `load_validated` and `verify` re-exports.
- **Evidence:** `crates/pavis-pvs/src/lib.rs` (`pub use verify::{load, load_validated, verify}`).
- **Assessment (Reason):** Encourages callers to rely on semantic validation in the integrity layer.
- **Recommendation (Suggestion):** Remove or restrict `load_validated` from public API.
- **Doc Drift?:** No.

#### F-2: Internal routing representation public
- **Expectation:** Runtime routing internals remain crate-private.
- **Observed:** `CompiledVirtualHost` is public.
- **Evidence:** `crates/pavis/src/router.rs` (`pub struct CompiledVirtualHost`).
- **Assessment (Reason):** Exposes runtime internals and couples external code to implementation details.
- **Recommendation (Suggestion):** Reduce visibility to `pub(crate)` and expose stable API if needed.
- **Doc Drift?:** No.

#### F-3: Internal counter type public
- **Expectation:** Internal load-balancing state stays private.
- **Observed:** `AlignedCounter` is public but used internally.
- **Evidence:** `crates/pavis/src/upstream/cluster.rs` (`pub struct AlignedCounter`).
- **Assessment (Reason):** Public API surface includes implementation detail.
- **Recommendation (Suggestion):** Reduce visibility to module or crate.
- **Doc Drift?:** No.

#### F-4: Request context type public
- **Expectation:** Request context type should be private unless part of stable API.
- **Observed:** `RouterContext` is public and re-exported.
- **Evidence:** `crates/pavis/src/proxy/context.rs` and `crates/pavis/src/proxy.rs`.
- **Assessment (Reason):** External coupling to runtime internals.
- **Recommendation (Suggestion):** Make `RouterContext` crate-private unless stable external API is required.
- **Doc Drift?:** No.

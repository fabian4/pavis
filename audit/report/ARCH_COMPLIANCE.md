## 📌 Overall Summary (Latest)

🚫 Blocker: 0 · 🔥 High: 0 · ⚠️ Medium: 1 · 🧹 Low: 0 · ✅ Resolved: 2

---

## Open Findings (Prioritized)

| ID  | Severity | Area | Short Title |
|----:|:--------:|------|-------------|
| F-1 | Medium | Relay Plugin | Relay lacks ingest/codec plugin dependencies |

---

## Review Entry — 2025-12-30T04:15:22Z

### Scope
- Repository-wide architecture compliance scan.

---

### Method
- Cross-check of `Architecture.md` boundaries against code structure and responsibilities.


### Model
- gemini-2.0-flash-exp

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | Medium | Relay Plugin | Relay lacks ingest/codec plugin dependencies | Open |

---

### Detailed Findings

#### F-1: Relay lacks ingest/codec plugin dependencies
- **Expectation:** `Architecture.md` (Sec 3.x) requires `pavis-relay` to support compile-time inclusion of ingest/codec pipelines via Cargo features.
- **Observed:** `crates/pavis-relay/Cargo.toml` has no dependencies on `pavis-codec-*` or `pavis-ingest-*`.
- **Evidence:** `crates/pavis-relay/Cargo.toml` dependencies list.
- **Assessment (Reason):** Relay cannot function as the control plane orchestrator as defined in the architecture without these plugins.
- **Recommendation (Suggestion):** Add `pavis-codec-*` and `pavis-ingest-*` as optional dependencies in `pavis-relay` and implement the feature flag selection logic.
- **Doc Drift?:** No — implementation is lagging behind architecture.

---

> Older review entries continue below this point, in reverse chronological order.

## Review Entry — 2025-12-30T03:18:52Z

### Scope
- Targeted boundary review for routing model storage in `crates/pavis-core` and runtime wrappers in `crates/pavis`.

---

### Method
- Focused file inspection of routing model definitions and runtime wrapper usage.


### Model
- GPT-5

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | Low | Core routing model | Runtime-only regex state removed from core model | Done |

---

### Detailed Findings

#### F-1: Runtime-only regex state removed from core model
- **Expectation:** Core models in `pavis-core` remain canonical and avoid runtime-only state.
- **Observed:** Runtime compiled regex storage moved to runtime wrapper types.
- **Evidence:** `crates/pavis-core/src/runtime/routing.rs` no longer defines `compiled_regex`; runtime compilation now stored in `crates/pavis/src/router.rs`.
- **Assessment (Reason):** Keeps layering intact and removes runtime-only details from the core public model.
- **Recommendation (Suggestion):** None.
- **Doc Drift?:** No — change aligns with documented layering rules.

---

## Review Entry — 2025-12-30T03:08:16Z

### Scope
- Targeted boundary review of `.pvs` inspection responsibilities in `crates/pavis-relay`.

---

### Method
- Focused file inspection of relay `.pvs` parsing/validation call sites.


### Model
- GPT-5

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | Medium | Relay PVS boundary | Relay now delegates `.pvs` integrity checks to `pavis-pvs` | Done |

---

### Detailed Findings

#### F-1: Relay now delegates `.pvs` integrity checks to `pavis-pvs`
- **Expectation:** `.pvs` inspection (magic/version/checksum) is owned by `pavis-pvs` only.
- **Observed:** Relay uses `pavis_pvs::verify` and `pavis_pvs::inspect` instead of local parsing.
- **Evidence:** `crates/pavis-relay/src/handlers.rs` uses `pavis_pvs::verify` and `pavis_pvs::inspect`; relay-local parsing module removed.
- **Assessment (Reason):** Restores the integrity boundary and avoids divergent parsing logic.
- **Recommendation (Suggestion):** None; keep relay orchestration-only.
- **Doc Drift?:** No — change aligns with architecture responsibilities.

---

## Review Entry — 2025-12-29T17:42:57Z

### Scope
- Repository-wide architecture compliance scan.

---

### Method
- Cross-check of `Architecture.md` boundaries against code structure and responsibilities.


### Model
- GPT-5

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | Medium | Relay PVS boundary | Relay parses `.pvs` headers outside `pavis-pvs` | Open |
| F-2 | Low | Core routing model | Core model stores runtime-only compiled regex | Open |

---

### Detailed Findings

#### F-1: Relay parses `.pvs` headers outside `pavis-pvs`
- **Expectation:** `.pvs` inspection (magic/version/checksum) is owned by `pavis-pvs` only.
- **Observed:** Relay implements its own header parsing and validation.
- **Evidence:** `crates/pavis-relay/src/pvs.rs` implements `parse_header` and `validate` for magic/version/checksum.
- **Assessment (Reason):** Boundary drift risks divergent integrity validation.
- **Recommendation (Suggestion):** Replace relay-local parsing with `pavis-pvs` helpers (e.g., `read_header`/`verify`) and keep relay orchestration-only.
- **Doc Drift?:** No — architecture explicitly assigns integrity checks to `pavis-pvs`.

#### F-2: Core model stores runtime-only compiled regex
- **Expectation:** Runtime-only fields live in runtime wrappers; core holds canonical semantics only.
- **Observed:** Core `Route` includes `compiled_regex: Option<regex::Regex>`.
- **Evidence:** `crates/pavis-core/src/runtime/routing.rs` defines `Route::compiled_regex`.
- **Assessment (Reason):** Couples core to runtime details and expands the public API surface with a runtime-only type.
- **Recommendation (Suggestion):** Move compiled regex storage to runtime wrapper structs, leaving core `Route` purely declarative.
- **Doc Drift?:** No — architecture defines core as canonical-only.
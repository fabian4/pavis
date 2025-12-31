# Code Change Checklist

This checklist is derived from the core audit tasks (1–11) defined in `agent/Tasks.md`. Use it to verify every code change before completion.

### 1. Architecture & Layering (Tasks 1, 6, 10)
- **Layering:** Does this change respect the strict dependency direction (e.g., `pavis-core` is foundational; runtime must not depend on codecs)?
- **Boundaries:** Are responsibilities in the right crate (`pavis-core` for semantics, `pavis-pvs` for binary integrity, `pavis-codec-*` for DTOs)?
- **Visibility:** Is the public API minimal? Use `pub(crate)` or `pub(super)` instead of `pub` where possible to avoid unnecessary coupling.
- **Dependency Graph:** Ensure no new cross-layer dependencies or unnecessary heavy crates were added.

### 2. Code Structure & Quality (Tasks 4, 7, 8)
- **Single Responsibility:** Is the code split logically by feature?
    - **Large Files:** Avoid production files exceeding 500 lines. Split by responsibility (types, business logic, I/O) into sub-modules within a directory (e.g., `agent/mod.rs` with `agent/worker.rs`).
    - **Large Functions:** Are functions concise? Extract complex logic into private helpers if a function exceeds ~50 lines or performs multiple distinct steps.
- **Testability:**
    - **Unit Tests:** Does every non-trivial function or logic block have a corresponding unit test (either in a `tests` module or a sibling `tests.rs`)?
    - **Seams:** Are there clear boundaries/traits for I/O and external state to allow for deterministic testing?
- **Duplication:** Have I introduced repeated patterns that should be consolidated into shared utilities or test helpers?
- **Comments:** Are comments technically accurate and meaningful (focusing on *why*, not *what*)? Are they free of grammar/spelling errors?
- **Standards:** Have I run `make fmt` and `make lint`?

### 3. Testing & Verification (Task 5)
- **Coverage:** Are there unit tests for core logic, edge cases, and error paths?
- **Integration/E2E:** If this affects system behavior, have the relevant tests in `crates/pavis-e2e` or `tests/` been updated?
- **CI Readiness:** Does `make ci-local` pass successfully?

### 4. Security & Safety (Task 9)
- **Secrets:** Are there any hardcoded keys, tokens, or sensitive information?
- **Unsafe:** If `unsafe` was used, is there a documented safety invariant?
- **Input Validation:** Is external/binary data validated (e.g., `rkyv::check_bytes`, magic bytes, or checksums)?

### 5. Performance & Allocations (Task 11)
- **Allocations:** Have I avoided unnecessary `.clone()`, `.to_string()`, or temporary buffer allocations in hot paths?
- **Async Efficiency:** Ensure no blocking operations are introduced in latency-sensitive async paths.

### 6. Documentation & Roadmap (Tasks 2, 3)
- **Roadmap Alignment:** Does this change conflict with planned items in `ROADMAP.md`?
- **Status Updates:** Does `ROADMAP.md` or any audit report in `audit/report/` need a status update (with UTC timestamp)?
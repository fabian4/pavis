# AI Agent Instructions

## References

| Document                                                                       | Description                                   |
| ------------------------------------------------------------------------------ | --------------------------------------------- |
| [README.md](./README.md)                                                       | Project overview and quick start              |
| [Architecture.md](./Architecture.md)                                           | System design and protocol details            |
| [ROADMAP.md](./ROADMAP.md)                                                     | Development phases and progress               |
| [Cargo.toml](./Cargo.toml)                                                     | Workspace configuration and dependencies      |
| [audit/report/ARCH_COMPLIANCE.md](audit/report/ARCH_COMPLIANCE.md)               | Architecture compliance review report         |
| [audit/report/ARCH_ROADMAP_ALIGNMENT.md](audit/report/ARCH_ROADMAP_ALIGNMENT.md) | Architecture vs roadmap alignment report      |
| [audit/report/ROADMAP_REVIEW.md](audit/report/ROADMAP_REVIEW.md)                 | Roadmap vs implementation review report       |
| [audit/report/STRUCTURE_REVIEW.md](audit/report/STRUCTURE_REVIEW.md)             | Rust code structure & file size review report |
| [audit/report/TEST_COVERAGE_REVIEW.md](audit/report/TEST_COVERAGE_REVIEW.md)     | Test coverage & quality review report         |
| [audit/report/PUBLIC_API_REVIEW.md](audit/report/PUBLIC_API_REVIEW.md)           | Public API & boundary stability review report |
| [audit/report/COMMENT_REVIEW.md](audit/report/COMMENT_REVIEW.md)                 | Code comment quality review report            |
| [audit/report/DUPLICATION_REVIEW.md](audit/report/DUPLICATION_REVIEW.md)         | Duplication & redundancy review report        |
| [audit/report/SECURITY_REVIEW.md](audit/report/SECURITY_REVIEW.md)               | Security review report                        |
| [audit/report/DEPENDENCY_BOUNDARY_REVIEW.md](audit/report/DEPENDENCY_BOUNDARY_REVIEW.md) | Dependency boundary review report       |
| [audit/report/PERFORMANCE_REVIEW.md](audit/report/PERFORMANCE_REVIEW.md)         | Performance & allocation review report        |

## Agent Review

- Follow the current format used in each existing report file under `audit/report/` when updating or adding entries.
- Every review entry must include a dedicated model note that states the exact model used to generate that entry.
- When completing a task, add a new history entry to the relevant review report instead of editing prior entries.
- After completing any review task, update the relevant review report and regenerate `audit/README.md`.
- `audit/README.md` is the single top-level status summary for the codebase (audits, roadmap overview, coverage).

## Agent Audit Tasks & Reporting

### Directory & Files

- All audit reports live under `audit/report/` (create the directory if missing).
- Report files (one task per file):
  - `audit/report/ARCH_COMPLIANCE.md`
  - `audit/report/ARCH_ROADMAP_ALIGNMENT.md`
  - `audit/report/ROADMAP_REVIEW.md`
  - `audit/report/STRUCTURE_REVIEW.md`
  - `audit/report/TEST_COVERAGE_REVIEW.md`
  - `audit/report/PUBLIC_API_REVIEW.md`
  - `audit/report/COMMENT_REVIEW.md`
  - `audit/report/DUPLICATION_REVIEW.md`
  - `audit/report/SECURITY_REVIEW.md`
  - `audit/report/DEPENDENCY_BOUNDARY_REVIEW.md`
  - `audit/report/PERFORMANCE_REVIEW.md`

## Task Summary

1. Architecture compliance vs `Architecture.md`.
2. Architecture vs roadmap alignment (`Architecture.md` + `ROADMAP.md`).
3. Roadmap vs implementation alignment (`ROADMAP.md` vs code).
4. Rust structure and file size organization review.
5. Test coverage and quality review (unit, integration, E2E).
6. Public API and boundary stability review.
7. Code comment quality review.
8. Duplication and redundancy review.
9. Security review (dependencies, unsafe, secrets).
10. Dependency boundary review (crate graph hygiene).
11. Performance and allocation hotspots review.

### Task 1: Architecture Compliance Review

- Review the entire repository against `Architecture.md`.
- Focus on structural, layering, boundary, and responsibility violations.
- Actively search for mismatches between what the architecture specifies and what the code actually implements.
- For each inconsistency (code vs `Architecture.md`):
  - State what the architecture specifies or intends.
  - State what the current code actually does (with evidence).
  - Explain the deviation clearly and concretely.
  - Classify the inconsistency:
    - Implementation bug / wrong design
    - Acceptable deviation with justification
    - Documentation drift (`Architecture.md` outdated or incomplete)
  - Be explicit that not all mismatches imply code is wrong; sometimes the doc must be updated (but do not modify `Architecture.md` in this task).
- Output a structured analysis report with findings and severity.
- Write the report to `audit/report/ARCH_COMPLIANCE.md` and keep it updated over time.

### Task 2: Architecture vs Roadmap Consistency Review

- Review `Architecture.md` and `ROADMAP.md` together for alignment.
- Produce a structured report with:
  - Roadmap items that conflict with architectural constraints.
  - Roadmap items that assume components/responsibilities not defined in `Architecture.md`.
  - Gaps where `Architecture.md` defines responsibilities not tracked in the roadmap.
- For each inconsistency:
  - Quote or summarize the architectural expectation.
  - Quote or summarize the roadmap item.
  - Explain the incompatibility clearly.
  - Propose a roadmap adjustment that preserves the architecture.
- Update `doc/ROADMAP.md` accordingly, without altering `Architecture.md`.
- Output the report to `audit/report/ARCH_ROADMAP_ALIGNMENT.md` and keep it updated over time.

### Task 3: Roadmap vs Implementation Review

- Review the entire repository against `ROADMAP.md`.
- Build a sectioned report grouped by roadmap phase and component.
- Identify and list:
  - Items marked planned but already implemented (with file paths as evidence).
  - Items marked in-progress but missing/incomplete (with file paths showing gaps).
  - Items implemented but not reflected in the roadmap (with file paths).
  - Status mismatches in summary tables or phase headers.
- For each finding, include:
  - Roadmap item text.
  - Observed implementation evidence (paths / code locations).
  - Suggested status change and rationale.
- Update `doc/ROADMAP.md` to reflect reality once evidence is collected.
- Output the report to `audit/report/ROADMAP_REVIEW.md` and keep it updated over time.

### Task 4: Rust Code Structure & File Size Review

- Review all Rust code in the repository, including test code.
- Evaluate whether code is split reasonably by feature and responsibility.
- Identify:
  - Overly long files.
  - Files containing multiple unrelated features.
  - Modules that violate single-responsibility or feature-based organization.
  - Repeated patterns that suggest missing shared utilities.
- Propose how to split/reorganize code strictly by feature/functionality:
  - Suggest module boundaries and naming.
  - Suggest crate boundary adjustments if needed (analysis only).
  - Suggest where tests should live after refactors.
- Output a structured report describing problems and recommended refactors.
- Analysis only: do not refactor code unless explicitly requested.
- Write the report to `audit/report/STRUCTURE_REVIEW.md` and keep it updated over time.

### Task 5: Test Coverage & Quality Review (Unit + Integration + E2E)

- Review all test code across the repository:
  - Unit tests (`#[cfg(test)]`, module-level tests)
  - Integration tests (`tests/` directories)
  - E2E tests (e.g. `crates/pavis-e2e` or equivalent)

- Analyze whether tests adequately cover:
  - Core functionality and critical paths
  - Edge cases and boundary conditions
  - Error paths and failure modes
  - Regression-prone areas (parsing, validation, boundaries, codecs, I/O, concurrency)

- **Coverage analysis (mandatory for every run):**
  - Use existing coverage artifacts (e.g. `audit/coverage.md`, tarpaulin JSON / LCOV outputs).
  - Do not run coverage tooling unless explicitly instructed.
  - Use coverage data as **supporting evidence**, not as a hard quality gate:
    - Identify critical code paths with low or missing coverage.
    - Distinguish acceptable low coverage from risky or misleading gaps.
    - Correlate coverage gaps with missing or weak test categories.

- E2E-specific review must include:
  - Test code quality (helpers, fixtures, readability, determinism)
  - Test workflow and process (how E2E tests run locally and in CI)
  - Case design:
    - Realistic scenarios
    - Negative / failure cases
    - Mixed versions or compatibility scenarios (if applicable)
    - Configuration permutations
  - Boundary and limit testing:
    - Large inputs
    - Malformed configs
    - Timeouts and resource constraints
  - Flakiness risks (timing, network, ordering, randomness) and mitigation strategies

- Check whether important methods or features lack tests:
  - Identify missing test categories (unit vs integration vs E2E).
  - Propose where and how new tests should be added (without writing code unless requested).

- Evaluate existing tests for quality:
  - Are assertions meaningful and behavior-focused?
  - Are tests redundant or overlapping without added value?
  - Are tests incorrectly scoped or overly coupled to internal implementation details?

- CI / GitHub Workflow review:
  - Review existing GitHub Actions workflows related to testing.
  - If missing or insufficient, recommend a standard baseline that includes:
    - `fmt` (rustfmt)
    - `clippy` (warnings as errors where reasonable)
    - Unit + integration tests
    - E2E tests (preferably as a separate job)
    - Caching strategy (cargo registry + build artifacts)
  - Recommend trigger policies:
    - `pull_request` for fast feedback
    - `push` to `main` for full test suite
    - Scheduled (nightly) runs for heavier E2E or soak tests

- Output a detailed **Test Coverage & Quality Review** report following the current
  format used in `audit/report/TEST_COVERAGE_REVIEW.md`.
- Write and maintain the report in `audit/report/TEST_COVERAGE_REVIEW.md`.
- Ensure:
  - All unresolved test gaps appear in **Open Findings (Prioritized)**.
  - Resolved gaps are removed from Open Findings and preserved only in historical review entries.

### Task 6: Public API & Boundary Stability Review

- Review all public (`pub`) APIs across all crates.
- Identify public types/traits/functions/modules that:
  - Expose internal implementation details.
  - Violate intended architectural boundaries.
  - Create unnecessary coupling between layers/crates.
  - Leak types from deep layers into higher layers.
- Evaluate whether public APIs are minimal, intentional, stable, and versionable.
- Recommend:
  - Which APIs should be `pub(crate)` or moved behind facades.
  - Where to add "boundary types" to reduce coupling.
- Output a structured report with findings and recommendations.
- Write the report to `audit/report/PUBLIC_API_REVIEW.md` and keep it updated over time.

### Task 7: Code Comment Quality Review

- Review all code comments across the repository.
- Evaluate comments for:
  - Grammar and spelling correctness.
  - Semantic clarity and technical accuracy.
  - Redundancy / unnecessary verbosity.
  - Alignment with actual code behavior.
  - Whether comments repeat what the code trivially states.
- Identify comments that are outdated, misleading, or inaccurate.
- Recommend whether each problematic comment should be revised, simplified, or removed.
- Output a structured comment quality review report.
- Write the report to `audit/report/COMMENT_REVIEW.md` and keep it updated over time.
- Include a UTC timestamp in each new entry (Notes) every time it is updated.

### Task 8: Duplication & Redundancy Review

- Review the repository for duplication across:
  - Rust code (similar functions, repeated logic, repeated parsing/validation patterns)
  - Test utilities/fixtures (similar setup logic)
  - Docs (repeated or conflicting descriptions)
  - CI workflows (duplicated job steps that can be shared)
- For each duplication cluster:
  - Describe the repeated pattern.
  - Identify all locations (paths/modules).
  - Explain why it’s problematic (bug risk, inconsistency, maintenance cost).
  - Propose consolidation options:
    - shared module / crate utility
    - macro/helpers where appropriate
    - test harness shared helpers
    - doc canonicalization strategy
- Output a structured duplication review report with recommended dedup steps.
- Write the report to `audit/report/DUPLICATION_REVIEW.md` and keep it updated over time.

### Task 9: Security Review (Dependencies + Unsafe + Secrets)

- Scan for dependency risks, unsafe Rust usage, and potential secret leaks.
- Identify:
  - Known vulnerable, outdated, or unmaintained dependencies (based on repo notes or existing tooling output only).
  - Unsafe blocks or FFI boundaries that warrant extra review, including missing invariants or safety notes.
  - Accidental credential exposure in code or docs.
- Provide evidence (crate names, files, `Cargo.toml` sections, code references).
- Recommend mitigations without changing production code.
- Ensure unresolved issues appear in **Open Findings (Prioritized)** and resolved issues remain only in history.
- Output a structured security review report.
- Write the report to `audit/report/SECURITY_REVIEW.md` and keep it updated over time.

### Task 10: Dependency Boundary Review (Crate Graph Hygiene)

- Review crate dependencies for boundary violations or unnecessary coupling.
- Identify:
  - Cross-layer dependencies that violate architecture direction.
  - Heavy or unnecessary dependencies in core crates.
  - Missing feature flags or optional dependencies that should be optional.
  - `dev-dependencies` leaking into production code paths.
- Recommend adjustments without changing production code.
- Output a structured dependency boundary review report.
- Write the report to `audit/report/DEPENDENCY_BOUNDARY_REVIEW.md` and keep it updated over time.

### Task 11: Performance & Allocation Hotspots Review

- Review the repository for performance-critical paths and allocation behavior:
  - Startup path (initialization, config loading, mmap usage, warm-up costs).
  - Serialization/deserialization paths.
  - Parsing/validation logic.
  - Per-request or per-connection hot paths.
- Identify potential performance and allocation issues:
  - Excessive allocations, repeated parsing/serialization, inefficient data structures.
  - Unnecessary cloning/copying or temporary buffers.
  - Blocking operations in latency-sensitive paths.
- Provide concrete evidence (files, functions, call paths) and mitigation ideas without redesigning the architecture.
- Classify each issue as:
  - A correctness-level performance bug.
  - A scalability risk.
  - Or an optimization opportunity.
- Do NOT micro-optimize blindly:
  - Avoid speculative changes without evidence.
  - Prefer changes justified by architecture intent, benchmarks, or clear allocation patterns.
- Output a structured **Performance & Allocation Hotspots Review** report.
- Write and maintain the report in: `audit/report/PERFORMANCE_REVIEW.md`.
- Ensure:
  - Unresolved performance issues appear in **Open Findings (Prioritized)**.
  - Addressed or obsolete issues are removed from Open Findings and preserved only in historical review entries.

## Coordination & Multi-Agent Safety Rules

- Assume multiple agents may work on this repository concurrently.
- Scope discipline:
  - Only analyze, review, or modify files explicitly covered by the current task.
  - If changes are detected outside the current task scope (e.g., unrelated files, unexpected diffs):
    - Do not modify or revert them.
    - Do not attempt to reconcile or reason about their intent.
    - Record them briefly in the relevant report under a section such as "Out-of-Scope Changes Observed".
    - Ignore them for the rest of the task.
- Formatting, linting, and test execution:
  - Do not automatically run fmt, lint, clippy, tests, or CI workflows unless explicitly instructed.
  - If a task would normally require running formatting, linting, or tests to validate findings:
    - Mark the relevant findings or recommendations as "Pending Verification".
    - Clearly state what command(s) should be run and what is expected to be validated.
    - Assume the repository owner will run these checks manually.
- Never block or fail a task due to missing local execution of formatters, linters, or tests unless explicitly required.

## Workspace & Layering

- Strict responsibilities:
  - `pavis-core`: protocol + canonical semantics; canonical validation of `RuntimeConfig`; no I/O, parsing, or format concerns.
  - `pavis-codec-*`: input DTOs, source-specific defaults/validation, transforms to `pavis-core::RuntimeConfig`.
  - `pavctl`: I/O orchestration shell that invokes codecs.
  - `pavis-pvs`: the only place to read/inspect `.pvs`, do magic/version/checksum checks, and run rkyv byte validation; **binary integrity only** (no semantic validation); runtime must not touch archive internals.
  - `pavis` runtime: consumes **current-version** validated config; only defensive crash-safety checks; no parsing/serde/rkyv, no semantic validation or config decoding (normal runtime state allocation is fine); version mismatch is a hard error.
  - `pavis-relay`/`pavis-governor`: control-plane migration and re-emission of current-version `.pvs` artifacts after core validation.
- Dependency direction is one-way: `pavis-core` is foundational; codecs/producers depend on core; runtime depends on core; runtime must not depend on codecs/serde/rkyv. Shared domain types live in core.

## Modules & Structure

- Rust 2018+ layout: no `mod.rs`; use `<module>.rs` with submodules in `<module>/`.
- Keep `<module>.rs` focused on module structure and `pub use`; avoid business logic there.
- Split files by responsibility (data types vs business logic vs pvs/I/O) and to prevent circular deps—not by size alone.
- Extract shared, foundational data structs/enums into `types.rs`/`model.rs` or similar when used by multiple siblings; keep cohesive, local types in place to avoid import noise.
- Prefer minimal visibility (`pub(super)`, `pub(crate)`); do not widen for convenience.
- Preserve public APIs and crate boundaries; avoid new cross-layer dependencies. Keep diffs small and readable.

## General Rules

1. Read before writing—follow existing patterns.
2. Make minimal changes needed to solve the problem.
3. Use stable Rust only; avoid `#![feature(...)]`.
4. Respect manual edits—if a file changed since you last read it, preserve the user's updates.
5. No git commit/push; the user handles version control.
6. Follow `doc/CODE_REVIEW.md` for priorities and update statuses when tasks complete.
7. Backward compatibility is a lower concern (no public release yet) unless the user requests stability explicitly.
8. Do not create a new crate unless the user explicitly asks.
9. Do not change the struct of `RuntimeConfig` unless explicitly instructed.
10. Any time `ROADMAP.md` is updated, refresh the top summary section inside `ROADMAP.md`.

## Tooling & Validation

- After any Rust code change: run `make fmt`, `make lint`.
- Validate builds/tests with `make build test` or `make ci` after edits.

## Code Style

| Aspect             | Guideline                     |
| ------------------ | ----------------------------- |
| Formatting         | Follow `rustfmt`              |
| Errors (binaries)  | Use `anyhow`                  |
| Errors (libraries) | Use `thiserror`               |
| Logging            | Use `tracing`, not `println!` |
| Shared types       | Put in `pavis-core`           |

## Safety Requirements

- Validate all binary data with `rkyv::check_bytes` before use.
- Check magic bytes and version before loading `.pvs` files.
- Never trust external input without validation.

## Benchmarking

| Item        | Value                                |
| ----------- | ------------------------------------ |
| Location    | `bench/`                             |
| Command     | `make benchmark`                     |
| CI Workflow | `.github/workflows/bench.yaml`       |
| Reference   | [bench/README.md](./bench/README.md) |

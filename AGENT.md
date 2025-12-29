# AI Agent Instructions

## References

| Document                                                                       | Description                                   |
| ------------------------------------------------------------------------------ | --------------------------------------------- |
| [README.md](./README.md)                                                       | Project overview and quick start              |
| [Architecture.md](./Architecture.md)                                           | System design and protocol details            |
| [ROADMAP.md](./ROADMAP.md)                                                     | Development phases and progress               |
| [Cargo.toml](./Cargo.toml)                                                     | Workspace configuration and dependencies      |
| [agent/audit/ARCH_COMPLIANCE.md](agent/audit/ARCH_COMPLIANCE.md)               | Architecture compliance review report         |
| [agent/audit/ARCH_ROADMAP_ALIGNMENT.md](agent/audit/ARCH_ROADMAP_ALIGNMENT.md) | Architecture vs roadmap alignment report      |
| [agent/audit/ROADMAP_REVIEW.md](agent/audit/ROADMAP_REVIEW.md)                 | Roadmap vs implementation review report       |
| [agent/audit/STRUCTURE_REVIEW.md](agent/audit/STRUCTURE_REVIEW.md)             | Rust code structure & file size review report |
| [agent/audit/TEST_COVERAGE_REVIEW.md](agent/audit/TEST_COVERAGE_REVIEW.md)     | Test coverage & quality review report         |
| [agent/audit/PUBLIC_API_REVIEW.md](agent/audit/PUBLIC_API_REVIEW.md)           | Public API & boundary stability review report |
| [agent/audit/COMMENT_REVIEW.md](agent/audit/COMMENT_REVIEW.md)                 | Code comment quality review report            |

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

## Ongoing Review Tasks

## Review Report Rules

- Each review task must write to a single fixed file under `agent/audit/`.
- Reports are append-only; do not rewrite or delete historical reviews.
- The newest entry must be inserted at the top of `Historical Reviews`.
- Previously reported issues must be explicitly marked as `NEW`, `EXISTING`, or `DONE`.
- Resolved issues must be removed from `Active Findings` and recorded as `DONE` in history.
- `Active Findings` must only contain unresolved, currently relevant issues.
- Every finding must include concrete evidence (file, symbol, or behavior).
- Do not introduce new architectural ideas or redesigns in reports.

### Task 1: Architecture Compliance Review
- Review the entire repository against `Architecture.md`.
- Focus on structural, layering, and responsibility violations.
- For each inconsistency:
  - State what the architecture specifies or intends.
  - State what the current code actually does (with evidence).
  - Explain the deviation clearly and concretely.
- Output a structured analysis report with findings and severity.
- Write the report to `agent/audit/ARCH_COMPLIANCE.md` and keep it updated over time.

### Task 2: Architecture vs Roadmap Consistency Review
- Review `Architecture.md` and `ROADMAP.md` together for alignment.
- Produce a structured report with:
  - Items in the roadmap that conflict with architectural constraints.
  - Items in the roadmap that assume components or responsibilities not defined in Architecture.md.
  - Gaps where Architecture.md defines responsibilities that the roadmap does not track.
- For each inconsistency:
  - Quote or summarize the architectural expectation.
  - Quote or summarize the roadmap item.
  - Explain the incompatibility clearly.
  - Propose a roadmap adjustment that preserves the architecture.
- Update `ROADMAP.md` accordingly, without altering Architecture.md.
- Output the report to `agent/audit/ARCH_ROADMAP_ALIGNMENT.md` and keep it updated over time.

### Task 3: Roadmap vs Implementation Review
- Review the entire repository against `ROADMAP.md`.
- Build a sectioned report grouped by roadmap phase and component.
- Identify and list:
  - Items marked as planned but already implemented (with file paths as evidence).
  - Items marked as in-progress but missing or incomplete (with file paths showing gaps).
  - Items implemented but not reflected in the roadmap (with file paths).
  - Status mismatches in summary tables or phase headers.
- For each finding, include:
  - Roadmap item text.
  - Observed implementation evidence (file paths or code locations).
  - Suggested status change and rationale.
- Update `ROADMAP.md` to reflect reality once evidence is collected.
- Output the report to `agent/audit/ROADMAP_REVIEW.md` and keep it updated over time.

### Task 4: Rust Code Structure & File Size Review
- Review all Rust code in the repository, including test code.
- Evaluate whether code is split reasonably by feature and responsibility.
- Identify:
  - Overly long files.
  - Files containing multiple unrelated features.
  - Modules that violate single-responsibility or feature-based organization.
- Propose how to split or reorganize code strictly by feature/functionality.
- Output a structured report describing problems and recommended refactors.
- This task produces analysis only; do not refactor code unless explicitly requested.
- Write the report to `agent/audit/STRUCTURE_REVIEW.md` and keep it updated over time.

### Task 5: Test Coverage & Quality Review
- Review all test code in the repository.
- Analyze whether tests adequately cover:
  - Core functionality.
  - Edge cases and boundary conditions.
  - Error paths and failure modes.
- Check whether important methods or features lack tests.
- Evaluate whether existing tests are meaningful, redundant, or incorrectly scoped.
- Output a detailed test coverage and quality report.
- Write the report to `agent/audit/TEST_COVERAGE_REVIEW.md` and keep it updated over time.

### Task 6: Public API & Boundary Stability Review
- Review all public (`pub`) APIs across all crates.
- Identify public types, traits, functions, or modules that:
  - Expose internal implementation details.
  - Violate intended architectural boundaries.
  - Create unnecessary coupling between layers or crates.
- Evaluate whether public APIs are minimal, intentional, and stable.
- Output a structured report with findings and recommendations.
- Write the report to `agent/audit/PUBLIC_API_REVIEW.md` and keep it updated over time.

### Task 7: Code Comment Quality Review
- Review all code comments across the repository.
- Evaluate comments for:
  - Grammar and spelling correctness.
  - Semantic clarity and technical accuracy.
  - Redundancy or unnecessary verbosity.
  - Alignment with actual code behavior.
- Identify comments that are outdated, misleading, or restate obvious code.
- Recommend whether each problematic comment should be revised, simplified, or removed.
- Output a structured comment quality review report.
- Write the report to `agent/audit/COMMENT_REVIEW.md` and keep it updated over time.
- Include a UTC timestamp in the review entry (Notes) every time it is updated.

# Tasks (1–11)

## Navigation

- [Core Instructions](./AGENT.md)
- [Workflow](./Workflow.md)
- [Audit Overview](./AuditOverview.md)
- [Multi-Agent Rules](./MultiAgentRules.md)
- [Code Review](./CodeReview.md)

See [Workflow.md](./Workflow.md) for execution steps and [AuditOverview.md](./AuditOverview.md) for report rules.

## Task 1: Architecture Compliance Review

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
- Write the report to `../audit/report/ARCH_COMPLIANCE.md` and keep it updated over time.

## Task 2: Architecture vs Roadmap Consistency Review

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
- Update `ROADMAP.md` accordingly, without altering `Architecture.md`.
- Output the report to `../audit/report/ARCH_ROADMAP_ALIGNMENT.md` and keep it updated over time.

## Task 3: Roadmap vs Implementation Review

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
- Update `ROADMAP.md` to reflect reality once evidence is collected.
- Output the report to `../audit/report/ROADMAP_REVIEW.md` and keep it updated over time.

## Task 4: Rust Code Structure & File Size Review

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
- Write the report to `../audit/report/STRUCTURE_REVIEW.md` and keep it updated over time.

## Task 5: Test Coverage & Quality Review (Unit + Integration + E2E)

- Review all test code across the repository:
  - Unit tests (`#[cfg(test)]`, module-level tests)
  - Integration tests (`tests/` directories)
  - E2E tests (e.g. `crates/pavis-e2e` or equivalent)

- Analyze whether tests adequately cover:
  - Core functionality and critical paths
  - Edge cases and boundary conditions
  - Error paths and failure modes
  - Regression-prone areas (parsing, validation, boundaries, codecs, I/O, concurrency)

- Coverage analysis (mandatory for every run):
  - Use existing coverage artifacts (e.g. `../audit/coverage.md`, tarpaulin JSON / LCOV outputs).
  - Do not run coverage tooling unless explicitly instructed.
  - Use coverage data as supporting evidence, not as a hard quality gate:
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

- Output a detailed Test Coverage & Quality Review report following the current
  format used in `../audit/report/TEST_COVERAGE_REVIEW.md`.
- Write and maintain the report in `../audit/report/TEST_COVERAGE_REVIEW.md`.
- Ensure:
  - All unresolved test gaps appear in Open Findings (Prioritized).
  - Resolved gaps are removed from Open Findings and preserved only in historical review entries.

## Task 6: Public API & Boundary Stability Review

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
- Write the report to `../audit/report/PUBLIC_API_REVIEW.md` and keep it updated over time.

## Task 7: Code Comment Quality Review

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
- Write the report to `../audit/report/COMMENT_REVIEW.md` and keep it updated over time.
- Include a UTC timestamp in each new entry (Notes) every time it is updated.

## Task 8: Duplication & Redundancy Review

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
- Write the report to `../audit/report/DUPLICATION_REVIEW.md` and keep it updated over time.

## Task 9: Security Review (Dependencies + Unsafe + Secrets)

- Scan for dependency risks, unsafe Rust usage, and potential secret leaks.
- Identify:
  - Known vulnerable, outdated, or unmaintained dependencies (based on repo notes or existing tooling output only).
  - Unsafe blocks or FFI boundaries that warrant extra review, including missing invariants or safety notes.
  - Accidental credential exposure in code or docs.
- Provide evidence (crate names, files, `Cargo.toml` sections, code references).
- Recommend mitigations without changing production code.
- Ensure unresolved issues appear in Open Findings (Prioritized) and resolved issues remain only in history.
- Output a structured security review report.
- Write the report to `../audit/report/SECURITY_REVIEW.md` and keep it updated over time.

## Task 10: Dependency Boundary Review (Crate Graph Hygiene)

- Review crate dependencies for boundary violations or unnecessary coupling.
- Identify:
  - Cross-layer dependencies that violate architecture direction.
  - Heavy or unnecessary dependencies in core crates.
  - Missing feature flags or optional dependencies that should be optional.
  - `dev-dependencies` leaking into production code paths.
- Recommend adjustments without changing production code.
- Output a structured dependency boundary review report.
- Write the report to `../audit/report/DEPENDENCY_BOUNDARY_REVIEW.md` and keep it updated over time.

## Task 11: Performance & Allocation Hotspots Review

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
- Output a structured Performance & Allocation Hotspots Review report.
- Write and maintain the report in: `../audit/report/PERFORMANCE_REVIEW.md`.
- Ensure:
  - Unresolved performance issues appear in Open Findings (Prioritized).
  - Addressed or obsolete issues are removed from Open Findings and preserved only in historical review entries.

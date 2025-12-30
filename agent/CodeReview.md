# Code Review (Task 12)

## Navigation

- [Core Instructions](./AGENT.md)
- [Tasks](./Tasks.md)
- [Workflow](./Workflow.md)
- [Audit Overview](./AuditOverview.md)
- [Multi-Agent Rules](./MultiAgentRules.md)

## Task 12: Code Review (Uncommitted Changes or Last Commit)

- Review only the current change set, not the entire repository.
- The review target is:
  - Uncommitted changes in the working tree, OR
  - The most recent commit (e.g. `HEAD`), depending on what is present.

### Review Scope

- Focus on:
  - Correctness and logic errors
  - Architectural boundary violations (see “Workspace & Layering”)
  - API design and visibility (`pub` / `pub(crate)` decisions)
  - Error handling and edge cases
  - Readability and maintainability
- Do NOT re-audit unrelated areas of the codebase.
- Do NOT speculate about future features or roadmap items.

### Review Method

- Base the review strictly on diffs:
  - `git diff` (for uncommitted changes)
  - or `git show HEAD` / `git diff HEAD~1..HEAD` (for last commit)
- Treat unchanged code as context only.

### Output Rules

- Do NOT modify code unless explicitly requested.
- Do NOT write to `../audit/report/*`.
- Output the review as:
  - Inline comments (if supported), OR
  - A structured review note grouped by file and concern.

Each review comment SHOULD include:
- File path and line range
- What changed
- Why it may be problematic or noteworthy
- A concrete suggestion (or “Looks good” if appropriate)

### Boundary & Safety Checks

- Explicitly call out:
  - Violations of architectural boundaries
  - Core crate or `Cargo.toml` changes lacking proper justification
  - Test-driven changes that weaken core semantics

### Non-blocking Policy

- The review must be advisory unless explicitly asked to block or enforce.
- Style, naming, or micro-optimizations should be marked as “Nit” or “Suggestion”.

### Execution Constraints

- Do NOT run fmt, lint, tests, or benchmarks unless explicitly instructed.
- If validation is recommended, mark it as “Pending Verification” and list commands to run.

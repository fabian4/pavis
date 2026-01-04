# Audit Overview

## Navigation

- [Core Instructions](./AGENT.md)
- [Tasks](./Tasks.md)
- [Workflow](./Workflow.md)
- [Multi-Agent Rules](./MultiAgentRules.md)
- [Code Review](./CodeReview.md)

## Report Format & Update Rules

- Reports in `../audit/report/` already conform to their canonical templates; the current structure of each file is authoritative.
- Do not re-migrate, reformat, or restructure report files.
- Allowed changes to report files:
  - Append new review entries in the existing section order.
  - Update `Open Findings (Prioritized)` to reflect current open items.
- Historical entries must remain unchanged.
- After completing any review task, update the relevant report and regenerate `../audit/README.md`.
- `../audit/README.md` is the single top-level status summary for the codebase (audits, roadmap overview, coverage).
- Every new review entry must include explicit traceability fields:
  - Model used (exact identifier string)
  - UTC timestamp
- Do not add snapshot commit hashes to review entry headers.

## audit/README.md Generation Rules

- `../audit/README.md` is derived only from:
  - `Open Findings (Prioritized)` sections in `../audit/report/*.md`
  - Roadmap overview table extracted from `../ROADMAP.md`
  - Coverage summary extracted from `../audit/coverage.md` (if present)
- Missing inputs:
  - If a report is missing, omit it from the breakdown.
  - If `../audit/coverage.md` is missing, mark coverage as “Unavailable”.
  - If `../ROADMAP.md` is missing, mark the roadmap section as “Unavailable”.

## Allowed Writes (Scope Fence)

- Tasks 1, 4, 6, 7, 8, 9, 10, 11:
  - `../audit/report/<TASK_REPORT>.md`
  - `../audit/README.md`
- Task 5:
  - `../audit/report/TEST_COVERAGE_REVIEW.md`
  - `../audit/README.md`
  - Read-only evidence: `../audit/coverage.md`
- Tasks 2 and 3:
  - `../audit/report/<TASK_REPORT>.md`
  - `../audit/README.md`
  - `../ROADMAP.md` (only when the task explicitly requires updating it)
- Any other file modifications are out of scope.

## Directory & Files

- All audit reports live under `../audit/report/` (create the directory if missing).
- Report files (one task per file):
  - `../audit/report/ARCH_COMPLIANCE.md`
  - `../audit/report/ARCH_ROADMAP_ALIGNMENT.md`
  - `../audit/report/ROADMAP_REVIEW.md`
  - `../audit/report/STRUCTURE_REVIEW.md`
  - `../audit/report/TEST_COVERAGE_REVIEW.md`
  - `../audit/report/PUBLIC_API_REVIEW.md`
  - `../audit/report/COMMENT_REVIEW.md`
  - `../audit/report/DUPLICATION_REVIEW.md`
  - `../audit/report/SECURITY_REVIEW.md`
  - `../audit/report/DEPENDENCY_BOUNDARY_REVIEW.md`
  - `../audit/report/PERFORMANCE_REVIEW.md`

## Audit Tasks (1-11)

These tasks are executed sequentially or in parallel by agents. See [Tasks.md](./Tasks.md) for details.

1. Architecture compliance vs `ARCHITECTURE.md`.
2. Architecture vs roadmap alignment (`ARCHITECTURE.md` + `ROADMAP.md`).
3. Roadmap vs implementation check.
4. Rust structure and file size organization review.
5. Test coverage and quality review (unit, integration, E2E).
6. Public API and boundary stability review.
7. Code comment quality review.
8. Duplication and redundancy review.
9. Security review (dependencies, unsafe, secrets).
10. Dependency boundary review (crate graph hygiene).
11. Performance and allocation hotspots review.

See [Tasks.md](./Tasks.md) for full task specifications.

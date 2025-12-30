# Multi-Agent Rules

## Navigation

- [Core Instructions](./AGENT.md)
- [Tasks](./Tasks.md)
- [Workflow](./Workflow.md)
- [Audit Overview](./AuditOverview.md)
- [Code Review](./CodeReview.md)

## Isolation Rules (Concurrent Agents)

- Assume multiple agents may work on this repository concurrently.
- Scope discipline:
  - Only analyze, review, or modify files explicitly covered by the current task.
  - If changes are detected outside the current task scope (e.g., unrelated files, unexpected diffs):
    - Do not modify or revert them.
    - Do not attempt to reconcile or reason about their intent.
    - Record them briefly in the relevant report under a section such as "Out-of-Scope Changes Observed".
    - Ignore them for the rest of the task.

## Snapshot Use (Tasks 1–11)

- All scanning and analysis for audit tasks must occur inside a snapshot under `~/.temp`.
- Only report outputs may be copied back to the main working tree.
- See [AGENT.md](./AGENT.md) for the required snapshot workflow steps.

## Test and Tooling Policy

- Do not automatically run fmt, lint, clippy, tests, or CI workflows unless explicitly instructed.
- If a task would normally require running formatting, linting, or tests to validate findings:
  - Mark the relevant findings or recommendations as "Pending Verification".
  - Clearly state what command(s) should be run and what is expected to be validated.
- Never block or fail a task due to missing local execution of formatters, linters, or tests unless explicitly required.

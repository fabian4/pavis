# Workflow

## Navigation

- [Core Instructions](./AGENT.md)
- [Tasks](./Tasks.md)
- [Audit Overview](./AuditOverview.md)
- [Multi-Agent Rules](./MultiAgentRules.md)
- [Code Review](./CodeReview.md)

## Execution Steps

1. Read [AGENT.md](./AGENT.md) and identify the task scope.
2. For Tasks 1–11, create a snapshot under `~/.temp` and run all analysis inside it.
3. Gather evidence and draft findings.
4. Update the relevant report(s) per [AuditOverview.md](./AuditOverview.md).
5. Regenerate `../audit/README.md` after any audit/report update.
6. If `../ROADMAP.md` is updated (Task 2 or 3), refresh the roadmap summary section at the top of the file.

## Report Submission Guidelines

- Append new review entries in the existing section order.
- Update `Open Findings (Prioritized)` to reflect current unresolved items.
- Preserve historical entries unchanged.
- Include required traceability fields (model + UTC timestamp).
- Do not add snapshot commit hashes to review entry headers.

## Git Workflow Rules

- No git commit or push; the user handles version control.
- Never revert unrelated changes or rewrite history.
- Avoid destructive commands unless explicitly requested.

## Tooling & Validation

- After any Rust code change: run `make fmt`, `make lint`.
- Validate builds/tests with `make build test` or `make ci` after edits.
- If validation is not run, mark it as "Pending Verification" and provide the commands to execute.

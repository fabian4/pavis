# Agent Operations Manual

## Navigation

- [Core Instructions](./README.md)
- [Checklist](./CHECKLIST.md)

---

## 1. Standard Workflow

### Git Workflow Rules
- **No Direct Pushes**: The user handles the final push and merge.
- **No Destructive Commands**: Never revert unrelated changes or rewrite history.
- **Tooling**:
  - Validate with `make ci-local` after any Rust code change.
  - If local validation isn't possible, mark as "Pending Verification" in the report.

---

## 2. Multi-Agent Concurrency Rules

### Isolation
- Assume multiple agents are working concurrently.
- **Scope Discipline**: Only modify files explicitly covered by your task.
- **Foreign Changes**: If you detect unexpected diffs/files:
  - Do not modify or revert them.
  - Record them as "Out-of-Scope Changes Observed".
  - Ignore them for your task.
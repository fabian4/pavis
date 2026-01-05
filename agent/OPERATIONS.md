# Agent Operations Manual

## Navigation

- [Core Instructions](./README.md)
- [Audit Protocol](./AUDIT.md)
- [Code Review](./REVIEW.md)
- [Checklist](./CHECKLIST.md)

---

## 1. Standard Workflow

### Execution Steps
1. Read [README.md](./README.md) and identify the task scope.
2. For Tasks 1–11 (Audit), create a snapshot under `~/.temp` and run all analysis inside it.
3. For Feature/Refactor tasks, follow the **Worktree Isolation** rules below.
4. Gather evidence and draft findings.
5. Update the relevant report(s) or code.
6. Regenerate `../audit/README.md` after any audit/report update.
7. If `../ROADMAP.md` is updated, refresh the roadmap summary section at the top of the file.

### Git Workflow Rules
- **No Direct Pushes**: The user handles the final push and merge.
- **No Destructive Commands**: Never revert unrelated changes or rewrite history.
- **Tooling**:
  - Run `make fmt` and `make lint` after any Rust code change.
  - Validate with `make build test` or `make ci`.
  - If local validation isn't possible, mark as "Pending Verification" in the report.

---

## 2. Worktree Isolation (SOP)

To maintain task isolation and prevent interference, agents must use `git worktree`.

### Setup
1. Identify a unique path (e.g., `/tmp/pavis-task-01`).
2. Create a new branch linked to the worktree:
   ```bash
   git worktree add <path_to_worktree> -b <task_branch_name>
   ```

### Usage Rules
- **Scope**: Operate *only* within the assigned worktree directory.
- **Context**: Ensure all tool calls are relative to the worktree root.
- **Persistence**: Commit changes locally within the worktree.
   ```bash
   git add .
   git commit -m "Task completion: <description>"
   ```

### Handover & Cleanup
1. Notify the user that the branch `<task_branch_name>` is ready.
2. Remove the worktree to clean up:
   ```bash
   git worktree remove <path_to_worktree>
   ```

---

## 3. Multi-Agent Concurrency Rules

### Isolation
- Assume multiple agents are working concurrently.
- **Scope Discipline**: Only modify files explicitly covered by your task.
- **Foreign Changes**: If you detect unexpected diffs/files:
  - Do not modify or revert them.
  - Record them as "Out-of-Scope Changes Observed".
  - Ignore them for your task.

### Snapshot Policy (Audits)
- All scanning/analysis for audit tasks (1-11) must occur inside a snapshot under `~/.temp`.
- Only final report files may be copied back to the main working tree.

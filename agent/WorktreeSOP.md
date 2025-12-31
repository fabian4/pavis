# Agent Task Isolation: Git Worktree Guideline

To maintain task isolation, prevent interference between concurrent agents, and ensure work is traceable, all agents must follow this `git worktree` execution guideline.

## 1. Git Worktree Setup

Each task must be executed in a dedicated worktree. This creates a separate directory for the task while sharing the same underlying git repository, allowing multiple agents to work concurrently on different branches without file-system conflicts.

**Steps to create a new worktree:**
1. Identify a unique path for the task (e.g., in a temporary directory).
2. Create a new branch and link it to the worktree:
   ```bash
   git worktree add <path_to_new_worktree> -b <task_branch_name>
   ```
   *Example:* `git worktree add /tmp/task-123 -b feat/xds-codec`

## 2. Worktree Usage

Agents must operate strictly within their assigned worktree. 

- **Isolation**: Do not make changes to the main repository or other active worktrees.
- **Operations**: All code modifications, security scans, and report generation must occur inside the task-specific directory.
- **Context**: Ensure all tool calls (shell commands, file reads) are relative to the worktree root.

## 3. Committing Changes

Upon task completion, the agent is responsible for persisting the work locally within the task branch.

**Guidelines for commits:**
1. Stage all relevant changes (reports, code, etc.).
2. Commit with a clear, descriptive message:
   ```bash
   git commit -m "Task completion: <brief_description>"
   ```

## 4. Finalizing Task (User Handover)

Agents are **prohibited** from pushing to a remote repository or merging the task branch back into `main`. The final push and merge are the responsibility of the user.

**Handover Process:**
1. Complete all local commits in the task worktree as described in Section 3.
2. Notify the user that the task is complete and the local branch `<task_branch_name>` is ready for review.
3. Provide a summary of the changes and the results of the verification step (Section 7).

## 5. Task Isolation & Scope Control
...
## 6. Cleanup

To maintain system hygiene, agents should remove their temporary worktree immediately after completing the local commits and notifying the user.

**Cleanup command:**
```bash
git worktree remove <path_to_worktree>
```
*Note: This removes the local task directory. The task branch remains available in the local git repository for the user to review and handle further.*

## 7. Verification

Before proposing a merge, the agent must verify the integrity of the worktree:
1. Run standard quality checks: `make fmt`, `make lint`, `make build`.
2. Run relevant tests: `make test` or specific crate tests.
3. If verification requires manual steps or environment-specific resources not available to the agent, mark the changes as **"Pending Verification"** in the report and provide the exact commands required for the user to verify the work.

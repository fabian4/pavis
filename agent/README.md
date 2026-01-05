# Pavis Agent Core Instructions

This directory contains the operational protocols for AI agents working on the Pavis codebase.

## Navigation

| Document | Purpose |
|----------|---------|
| [**Operations Manual**](./OPERATIONS.md) | Workflow, Worktree Isolation, Concurrency Rules. **Start Here.** |
| [**Checklist**](./CHECKLIST.md) | Quick reference for verifying code changes. |
| [**Specs**](./tasks/) | Feature specifications and implementation tasks. |


## Core Mandates

- **Isolation**: Never touch files outside your task scope.
- **Safety**: Never push to remote. Commit locally only.
- **Consistency**: Follow `ARCHITECTURE.md` strictly.

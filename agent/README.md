# Pavis Agent Core Instructions

This directory contains the operational protocols for AI agents working on the Pavis codebase.

## Navigation

| Document | Purpose |
|----------|---------|
| [**Operations Manual**](./OPERATIONS.md) | Workflow, Worktree Isolation, Concurrency Rules. **Start Here.** |
| [**Audit Protocol**](./AUDIT.md) | Definitions for Audit Tasks 1-11 and Reporting Rules. |
| [**Code Review**](./REVIEW.md) | Protocol for Task 12 (Code Review). |
| [**Checklist**](./CHECKLIST.md) | Quick reference for verifying code changes. |
| [**Specs**](./tasks/) | Feature specifications and implementation tasks. |

## Quick Start

1. **Understand the Goal**: Read the prompt and identify the Task ID (1-12) or Feature name.
2. **Setup**:
   - For Audits: Create a snapshot (see [Operations](./OPERATIONS.md)).
   - For Features: Create a worktree (see [Operations](./OPERATIONS.md)).
3. **Execute**: Follow the constraints in the relevant protocol file.
4. **Report**: Update the canonical reports in `../audit/report/`.

## Core Mandates

- **Traceability**: All findings must reference the Model ID and Timestamp.
- **Isolation**: Never touch files outside your task scope.
- **Safety**: Never push to remote. Commit locally only.
- **Consistency**: Follow `ARCHITECTURE.md` strictly.

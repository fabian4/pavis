# AI Agent Instructions (Core)

## Navigation

- [Tasks](./agent/Tasks.md)
- [Workflow](./agent/Workflow.md)
- [Audit Overview](./agent/AuditOverview.md)
- [Multi-Agent Rules](./agent/MultiAgentRules.md)
- [Code Review](./agent/CodeReview.md)

## References

| Document                                                                                  | Description                                   |
| ----------------------------------------------------------------------------------------- | --------------------------------------------- |
| [README.md](../README.md)                                                                 | Project overview and quick start              |
| [ARCHITECTURE.md](../ARCHITECTURE.md)                                                     | System design and protocol details            |
| [ROADMAP.md](../ROADMAP.md)                                                               | Development phases and progress               |
| [Cargo.toml](../Cargo.toml)                                                               | Workspace configuration and dependencies      |
| [audit/report/ARCH_COMPLIANCE.md](../audit/report/ARCH_COMPLIANCE.md)                     | Architecture compliance review report         |
| [audit/report/ARCH_ROADMAP_ALIGNMENT.md](../audit/report/ARCH_ROADMAP_ALIGNMENT.md)       | Architecture vs roadmap alignment report      |
| [audit/report/ROADMAP_REVIEW.md](../audit/report/ROADMAP_REVIEW.md)                       | Roadmap vs implementation review report       |
| [audit/report/STRUCTURE_REVIEW.md](../audit/report/STRUCTURE_REVIEW.md)                   | Rust code structure & file size review report |
| [audit/report/TEST_COVERAGE_REVIEW.md](../audit/report/TEST_COVERAGE_REVIEW.md)           | Test coverage & quality review report         |
| [audit/report/PUBLIC_API_REVIEW.md](../audit/report/PUBLIC_API_REVIEW.md)                 | Public API & boundary stability review report |
| [audit/report/COMMENT_REVIEW.md](../audit/report/COMMENT_REVIEW.md)                       | Code comment quality review report            |
| [audit/report/DUPLICATION_REVIEW.md](../audit/report/DUPLICATION_REVIEW.md)               | Duplication & redundancy review report        |
| [audit/report/SECURITY_REVIEW.md](../audit/report/SECURITY_REVIEW.md)                     | Security review report                        |
| [audit/report/DEPENDENCY_BOUNDARY_REVIEW.md](../audit/report/DEPENDENCY_BOUNDARY_REVIEW.md) | Dependency boundary review report          |
| [audit/report/PERFORMANCE_REVIEW.md](../audit/report/PERFORMANCE_REVIEW.md)               | Performance & allocation review report        |

## Audit System Overview

- Audits live under `../audit/report/`.
- The top-level status summary is `../audit/README.md`.
- Coverage evidence (if present) is `../audit/coverage.md`.
- See [AuditOverview.md](./AuditOverview.md) for report rules and update criteria.

## Core Code & Cargo Modification Guard (Test Safety Rule)

- Tests MUST adapt to the architecture, not the other way around.
- Core crates (`pavis-core`, `pavis-pvs`, `pavis-*-api`) MUST NOT be modified solely to make tests easier.
- `Cargo.toml` MUST NOT be modified solely to add test or mocking dependencies.
- Exceptions are allowed only with explicit justification:
  - correctness, safety, or architectural necessity
  - boundary check against “Workspace & Layering” rules
  - alternatives considered

## Coordination & Snapshot Workflow

- Assume multiple agents may work concurrently; keep scope tight and record out-of-scope changes.
- Follow [MultiAgentRules.md](./MultiAgentRules.md) for isolation requirements.

### Snapshot Workflow (Required for Tasks 1–11)

1. Create a clean, read-only snapshot at the current HEAD under `~/.temp`:
   - `TMP="$HOME/.temp/agent-snapshots/$(date +%s)"`
   - `mkdir -p "$TMP"`
   - `git clone --no-hardlinks . "$TMP/repo"`
   - `cd "$TMP/repo"`
   - `git status --porcelain` must be empty
   - Alternative: `git worktree add --detach "$TMP/repo" HEAD`
2. If the snapshot is not clean:
   - Record this under “Out-of-scope changes observed” in the relevant report entry.
   - Continue analysis; do not block the task.
3. Run all scanning and analysis only inside the snapshot.
4. Generate report artifacts inside the snapshot only.
5. Copy report outputs back into the main repo after analysis (reports and summaries only).
6. Never copy source code from the snapshot into the main working tree.

## Workspace & Layering

- Strict responsibilities:
  - `pavis-core`: protocol + canonical semantics; canonical validation of `RuntimeConfig`; no I/O, parsing, or format concerns.
  - `pavis-codec-*`: input DTOs, source-specific defaults/validation, transforms to `pavis-core::RuntimeConfig`.
  - `pavctl`: I/O orchestration shell that invokes codecs.
  - `pavis-pvs`: the only place to read/inspect `.pvs`, do magic/version/checksum checks, and run rkyv byte validation; **binary integrity only** (no semantic validation); runtime must not touch archive internals.
  - `pavis` runtime: consumes **current-version** validated config; only defensive crash-safety checks; no parsing/serde/rkyv, no semantic validation or config decoding (normal runtime state allocation is fine); version mismatch is a hard error.
  - `pavis-relay`/`pavis-governor`: control-plane migration and re-emission of current-version `.pvs` artifacts after core validation.
- Dependency direction is one-way: `pavis-core` is foundational; codecs/producers depend on core; runtime depends on core; runtime must not depend on codecs/serde/rkyv. Shared domain types live in core.

## Modules & Structure

- Rust 2018+ layout: no `mod.rs`; use `<module>.rs` with submodules in `<module>/`.
- Keep `<module>.rs` focused on module structure and `pub use`; avoid business logic there.
- Split files by responsibility (data types vs business logic vs pvs/I/O) and to prevent circular deps—not by size alone.
- Extract shared, foundational data structs/enums into `types.rs`/`model.rs` or similar when used by multiple siblings; keep cohesive, local types in place to avoid import noise.
- Prefer minimal visibility (`pub(super)`, `pub(crate)`); do not widen for convenience.
- Preserve public APIs and crate boundaries; avoid new cross-layer dependencies. Keep diffs small and readable.

## General Rules

1. Read before writing—follow existing patterns.
2. Make minimal changes needed to solve the problem.
3. Use stable Rust only; avoid `#![feature(...)]`.
4. Respect manual edits—if a file changed since you last read it, preserve the user's updates.
5. No git commit/push; the user handles version control.
6. Follow `doc/CODE_REVIEW.md` for priorities and update statuses when tasks complete.
7. Backward compatibility is a lower concern (no public release yet) unless the user requests stability explicitly.
8. Do not create a new crate unless the user explicitly asks.
9. Do not change the struct of `RuntimeConfig` unless explicitly instructed.
10. Any time `ROADMAP.md` is updated, refresh the roadmap summary section at the top of the file.

## Tooling & Validation

- After any Rust code change: run `make fmt`, `make lint`.
- Validate builds/tests with `make build test` or `make ci` after edits.

## Code Style

| Aspect             | Guideline                     |
| ------------------ | ----------------------------- |
| Formatting         | Follow `rustfmt`              |
| Errors (binaries)  | Use `anyhow`                  |
| Errors (libraries) | Use `thiserror`               |
| Logging            | Use `tracing`, not `println!` |
| Shared types       | Put in `pavis-core`           |

## Safety Requirements

- Validate all binary data with `rkyv::check_bytes` before use.
- Check magic bytes and version before loading `.pvs` files.
- Never trust external input without validation.

## Benchmarking

| Item        | Value                                |
| ----------- | ------------------------------------ |
| Location    | `bench/`                             |
| Command     | `make benchmark`                     |
| CI Workflow | `.github/workflows/bench.yaml`       |
| Reference   | [bench/README.md](../bench/README.md) |

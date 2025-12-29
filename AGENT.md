# AI Agent Instructions

## References

| Document | Description |
|----------|-------------|
| [README.md](./README.md) | Project overview and quick start |
| [Architecture.md](./Architecture.md) | System design and protocol details |
| [ROADMAP.md](doc/ROADMAP.md) | Development phases and progress |
| [CODE_REVIEW.md](doc/CODE_REVIEW.md) | Action plan and technical debt tracking |
| [Cargo.toml](./Cargo.toml) | Workspace configuration and dependencies |

## Workspace & Layering

- Strict responsibilities:
  - `pavis-core`: protocol + canonical semantics; canonical validation of `RuntimeConfig`; no I/O, parsing, or format concerns.
  - `pavis-codec-*`: input DTOs, source-specific defaults/validation, transforms to `pavis-core::RuntimeConfig`.
  - `pavctl`: I/O orchestration shell that invokes codecs.
  - `pavis-pvs`: the only place to read/inspect `.pvs`, do magic/version/checksum checks, and run rkyv byte validation; runtime must not touch archive internals.
  - `pavis` runtime: consumes trusted `RuntimeConfig`; only defensive crash-safety checks; no parsing/serde/rkyv, no semantic validation or config decoding (normal runtime state allocation is fine).
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

## Tooling & Validation

- After any Rust code change: run `make fmt`, `make lint`.
- Validate builds/tests with `make build test` or `make ci` after edits.

## Code Style

| Aspect | Guideline |
|--------|-----------|
| Formatting | Follow `rustfmt` |
| Errors (binaries) | Use `anyhow` |
| Errors (libraries) | Use `thiserror` |
| Logging | Use `tracing`, not `println!` |
| Shared types | Put in `pavis-core` |

## Safety Requirements

- Validate all binary data with `rkyv::check_bytes` before use.
- Check magic bytes and version before loading `.pvs` files.
- Never trust external input without validation.

## Benchmarking

| Item | Value |
|------|-------|
| Location | `bench/` |
| Command | `make benchmark` |
| CI Workflow | `.github/workflows/bench.yaml` |
| Reference | [bench/README.md](./bench/README.md) |

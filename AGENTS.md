# AI Agent Instructions

## References

- [README.md](./README.md) - Project overview and quick start
- [Architecture.md](./Architecture.md) - System design and protocol details
- [ROADMAP.md](./ROADMAP.md) - Development phases and progress
- [Cargo.toml](./Cargo.toml) - Workspace configuration and dependencies

## Rules

1. **Read before writing** - Understand existing code patterns before making changes
2. **Minimal changes** - Make the smallest possible modification to solve the problem
3. **No unstable features** - Use only stable Rust; avoid `#![feature(...)]`
4. **Format & Lint** - Run `cargo fmt --all` and `cargo clippy --all` after ANY Rust code change
5. **Validate changes** - Run `cargo build --workspace && cargo test --workspace` after edits
6. **Preserve structure** - Do not reorganize code unless explicitly requested
7. **No git commit/push** - Never run `git commit` or `git push`; user handles version control
8. **Respect manual edits** - If file content differs from your last read, user modified it; preserve their changes

## Code Style

- Follow `rustfmt` formatting
- Use `anyhow` for binaries, `thiserror` for libraries
- Use `tracing` for logging, not `println!`
- Shared types go in `pavis-core`

## Safety Requirements

- Validate all binary data with `rkyv::check_bytes` before use
- Check magic bytes and version before loading `.pvs` files
- Never trust external input without validation

## Benchmarking

- **Location:** `bench/`
- **Command:** `make benchmark` (runs all proxies, generates `results/summary.md`)
- **CI Workflow:** `.github/workflows/bench.yaml`
- **Reference:** See `bench/README.md` for matrix design and methodology

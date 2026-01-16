# AI Agent Instructions (Core)

## AI Reasoning & Execution Model
- The agent must reason and plan internally before acting.
- Internal reasoning steps are not fully exposed unless explicitly requested.
- The agent must strictly respect explicit constraints, dependency ordering, and risk.
- Reasoning must prioritize system integrity over speed of implementation.

## Decision Priority
When resolving conflicts or making implementation choices, the following priority order is absolute:
1. **Correctness and safety**: Functional correctness and memory safety (PVS validation, rkyv checks).
2. **Architecture boundaries and layering**: Strict adherence to crate responsibilities (Core vs. Codec vs. Runtime).
3. **Maintainability**: Readability, idiomatic Rust, and clarity for future human/AI developers.
4. **Performance**: Latency and throughput optimizations.
5. **Code size and local elegance**: Minimal diffs and concise logic.

## Task Complexity & Workflow

### 1. Classification
- **Trivial**: Documentation updates, single-line fixes, formatting, or renaming variables.
    - *Planning*: Not required.
    - *Confirmation*: Not required before execution.
- **Moderate**: Feature implementation within a single crate, refactoring multiple functions/files, or adding unit tests.
    - *Planning*: Required.
    - *Confirmation*: Required before entering Code mode.
- **Complex**: Cross-crate changes, core protocol modifications, architectural shifts, or new crate creation.
    - *Planning*: Required (high-depth analysis).
    - *Confirmation*: Required before entering Code mode.

### 2. Plan / Code Workflow
- **Plan Mode**: Responsible for discovery, impact analysis, and technical strategy.
    - A Plan must include: Affected files, logic changes, dependency impacts, and verification steps.
    - Exit Condition: User approval of the proposed strategy. For cases where a single, clearly superior strategy is proposed and the user clearly accepts it, this constitutes approval.
- **Code Mode**: Responsible for implementation, testing, and CI validation.
    - Trigger: Approval of the Plan for Moderate/Complex tasks, or direct identification of a Trivial task.
    - Responsibility: Atomic application of changes and adherence to the Code Change Checklist.

## Missing Information & Self-Correction

### 1. Handling Missing Information
- Progress must not be stalled by minor, cosmetic, or non-material uncertainties.
- If logs, errors, or repository context are partial, the agent should proceed using **explicit assumptions**.
- Assumptions must be stated clearly before acting.
- Clarification is mandatory ONLY when missing information would materially affect the chosen architectural or strategic decision.

### 2. Self-Correction Rule (Mandatory)
- The agent MUST fix low-level mistakes it introduced (syntax errors, missing imports, formatting issues, obvious compile failures) immediately and without asking for permission.
- Only high-risk, irreversible, or wide-impact changes (e.g., deleting data, altering public API signatures to fix a bug) require confirmation before correction.

## References

| Document                      | Description                              |
| ----------------------------- | ---------------------------------------- |
| [README.md](../README.md)     | Project overview and quick start         |
| [ARCHITECTURE.md](../ARCHITECTURE.md) | System design and protocol details |
| [ROADMAP.md](../ROADMAP.md)   | Development phases and progress          |
| [Cargo.toml](../Cargo.toml)   | Workspace configuration and dependencies |

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
6. Code review priority file is deprecated; no action required.
7. Adhere to execution plans in `docs/plan/**` (considered the temporary path for execution plans). Always update the task status in these plan files when a task is achieved.
8. Backward compatibility is a lower concern (no public release yet) unless the user requests stability explicitly.
9. Do not create a new crate unless the user explicitly asks.
10. Do not change the struct of `RuntimeConfig` unless explicitly instructed.
11. Any time `ROADMAP.md` is updated, refresh the roadmap summary section at the top of the file.

## Tooling & Validation

- After any Rust code change: run `make ci-local`.

## Execution Planning & Task Tracking

- **Location**: All active execution plans MUST reside in `docs/plan/**`.
- **Adherence**: Agents MUST strictly follow the steps outlined in the execution plan.
- **Status Updates**: Agents MUST update the task status (e.g., `[ ]` to `[x]`) in the relevant plan file as soon as a task is completed.

## Zero-Option Runtime Philosophy

Every agent MUST adhere to the "Zero-Option" configuration philosophy when modifying `pavis-core` or the `pavis` runtime.

### The Rules
1. **No Ambiguous Options**: Avoid `Option<T>` for feature toggles or policy configuration. Use explicit Enums instead (e.g., `enum TlsConfig { Disabled, Enabled { .. } }`).
2. **Strong Typing**: Use domain-specific newtypes (e.g., `Path`, `Hostname`, `UpstreamName`) instead of primitive types like `String` or `u32` for configuration fields.
3. **Materialized Defaults**: The **Codec** layer is responsible for resolving "missing" user input into a concrete decision. By the time configuration reaches `pavis-core`, all defaults MUST be explicit.
4. **No Runtime Inference**: The runtime MUST NOT guess or apply defaults (e.g., "if timeout is missing, use 5s"). It must execute the configuration exactly as provided in the `.pvs` artifact.

### Rationale
- **Illegal States**: Explicit enums make invalid configurations (like `tls_enabled: true` but `cert_path: None`) structurally unrepresentable.
- **Ambiguity**: `Option::None` is semantically overloaded (disabled vs. use default). Explicit variants remove this ambiguity.
- **Optimization**: Specific enums allow `rkyv` to generate a more efficient and deterministic memory layout for the binary protocol.
- **Separation of Concerns**: Policy (defaults) lives in the Codec; Mechanism (execution) lives in the Runtime.

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

# Code Change & Readability Checklist

Derived from Rust Readability Standards. Verify before completion.

### 1. Architecture & Layering
- [ ] **Layering**: Dependency direction respected (Core -> PVS -> Runtime)?
- [ ] **Boundaries**: Code in the correct crate (e.g., semantic logic in `core`, integrity in `pvs`)?
- [ ] **Visibility**: Is the public API minimal? (`pub(crate)` preferred over `pub`).
- [ ] **Abstraction**: Are type systems used effectively (enums/structs) without over-abstraction?

### 2. File & Module Structure
- [ ] **Module Division**: Does each module have a clear, single responsibility?
- [ ] **Size**: Production files < 600 lines? (Review for split if approaching limit).
- [ ] **Organization**: No `mod.rs` files (use Rust 2018+ layout: `module.rs` + `module/`).
- [ ] **Consistency**: Unified naming conventions and hierarchical structure across the project?

### 3. Functions & Methods
- [ ] **Length**: Are functions concise? (Goal: < 30-50 lines).
- [ ] **Naming**: Concise, descriptive names using `snake_case`?
- [ ] **Parameters**: Manageable number of parameters? (Use structs for configuration).
- [ ] **Nesting**: Avoided deep nesting? (Use early returns and helper functions).
- [ ] **Ordering**: Logical method ordering (Constructors -> Operations -> Destructors)?

### 4. Variables & Constants
- [ ] **Naming**: Variables are descriptive; constants are `UPPERCASE_WITH_UNDERSCORES`.
- [ ] **Magic Numbers**: Replaced with descriptive constants or enums?
- [ ] **Lifecycle**: Variable lifecycles are clear; unnecessary clones avoided.

### 5. Readability & Style
- [ ] **Conciseness**: Avoided overly long or complex expressions?
- [ ] **Formatting**: Adheres to `rustfmt` standards (`make fmt`)?
- [ ] **Error Handling**: Minimal use of `unwrap()` or `expect()`? (Prefer `?` or explicit matching).
- [ ] **Control Flow**: Kept simple (simple `if`, `match`, and `loop` structures)?

### 6. Comments & Documentation
- [ ] **Doc Comments**: Important functions, structs, and modules have `///` or `//!` docs.
- [ ] **Value**: Do comments explain **why** (logic/intent) rather than **what** (obvious code)?
- [ ] **Safety**: Every `unsafe` block has a `// Safety:` comment documenting invariants.
- [ ] **Cleanup**: No lingering `TODO`, `FIXME`, or commented-out code blocks?

### 7. Testing & Verification
- [ ] **Coverage**: Core logic, edge cases, and error paths tested? (Target: 90%+).
- [ ] **Responsibility**: Each test function tests only a single unit of logic?
- [ ] **Placement**: Unit tests colocated; Integration tests in `tests/` directory?
- [ ] **CI Readiness**: `make ci-local` or `make build test` passes cleanly?

### 8. Performance & Safety
- [ ] **Allocations**: No unnecessary `.clone()` or `.to_string()` in hot paths?
- [ ] **Async**: No blocking operations (e.g., `std::fs`) in async contexts?
- [ ] **Secrets**: No hardcoded keys, tokens, or sensitive information?

### 9. Documentation & Roadmap
- [ ] **Roadmap**: Change is aligned with `ROADMAP.md`?

# Agent Operations Manual

## 1. Standard Workflow

### Execution Protocol
- **Classify Complexity**: Determine if the task is Trivial, Moderate, or Complex.
- **Plan Mode**: Required for Moderate and Complex tasks. Provide a detailed strategy and wait for approval.
- **Code Mode**: 
    - For Trivial tasks: Direct execution is allowed.
    - For Moderate/Complex: Only enter after Plan approval.
- **Unilateral Execution**: Strictly prohibited except for Trivial fixes or self-correction of agent-introduced low-level errors (per the Self-Correction Rule). Any other unilateral execution is a violation of this contract.

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

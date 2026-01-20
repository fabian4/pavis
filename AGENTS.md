# AI Agent Instructions (Core)

## Quick Reference: Task Workflow

```
┌─ Task Received
│
├─ Classify: Trivial / Moderate / Complex
│   │
│   ├─ Trivial
│   │   └─→ Execute → Validate (make ci-local)
│   │
│   └─ Moderate / Complex
│       └─→ Plan Mode (include doc updates) → Get User Approval → Code Mode →
│           Validate (make ci-local)
│
├─ Documentation updates are only required when explicitly requested by the user:
│   • ARCHITECTURE.md
│   • README.md
│   • ROADMAP.md
│   • docs/FEATURES.md
│   • Code docs (//!, ///, inline comments)
│   • Test docs (e2e/bench test rationale when tests change/add/remove)
│
└─ If issues found: Self-correct low-level errors immediately; escalate architectural issues
```

---

# Part I: Core Principles

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

## Zero-Option Runtime Philosophy

Every agent MUST adhere to the "Zero-Option" configuration philosophy when modifying `pavis-core` or the `pavis` runtime.

### The Rules

1. **No Ambiguous Options**: Avoid `Option<T>` for feature toggles or policy configuration. Use explicit Enums instead.
   - ❌ **BAD**: `tls_cert_path: Option<PathBuf>`
   - ✅ **GOOD**: `tls: TlsConfig::Enabled { cert_path: PathBuf }`

2. **Strong Typing**: Use domain-specific newtypes (e.g., `Path`, `Hostname`, `UpstreamName`) instead of primitive types like `String` or `u32` for configuration fields.

3. **Materialized Defaults**: The **Codec** layer is responsible for resolving "missing" user input into a concrete decision. By the time configuration reaches `pavis-core`, all defaults MUST be explicit.

4. **No Runtime Inference**: The runtime MUST NOT guess or apply defaults (e.g., "if timeout is missing, use 5s"). It must execute the configuration exactly as provided in the `.pvs` artifact.

### Rationale

- **Illegal States**: Explicit enums make invalid configurations (like `tls_enabled: true` but `cert_path: None`) structurally unrepresentable.
- **Ambiguity**: `Option::None` is semantically overloaded (disabled vs. use default). Explicit variants remove this ambiguity.
- **Optimization**: Specific enums allow `rkyv` to generate a more efficient and deterministic memory layout for the binary protocol.
- **Separation of Concerns**: Policy (defaults) lives in the Codec; Mechanism (execution) lives in the Runtime.

## References

| Document                      | Description                              |
| ----------------------------- | ---------------------------------------- |
| [README.md](../README.md)     | Project overview and quick start         |
| [ARCHITECTURE.md](../ARCHITECTURE.md) | System design and protocol details |
| [ROADMAP.md](../docs/project/roadmap.md)   | Development phases and progress          |
| [docs/project/features.md](../docs/project/features.md) | Feature status tracking |
| [Cargo.toml](../Cargo.toml)   | Workspace configuration and dependencies |

---

# Part II: Workflow & Process

## Task Complexity & Classification

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
    - A Plan must include: Affected files, logic changes, dependency impacts, verification steps, **and documentation updates required**.
    - Exit Condition: User approval of the proposed strategy. For cases where a single, clearly superior strategy is proposed and the user clearly accepts it, this constitutes approval.

- **Code Mode**: Responsible for implementation, testing, and CI validation. Documentation updates only when requested by the user.
    - Trigger: Approval of the Plan for Moderate/Complex tasks, or direct identification of a Trivial task.
    - Responsibility: Atomic application of changes and adherence to the Code Change Checklist.
    - **Completion Criteria**: Implementation is NOT complete until:
      1. Code changes are implemented and tested
      2. `make ci-local` passes successfully
      3. Any user-requested documentation updates are complete
      4. Any user-requested plan status updates are complete

## Execution Planning & Task Tracking

- **Location**: Execution plans in `docs/plan/**` are only used when explicitly requested by the user.
- **Adherence**: If a user requests a plan, follow the steps outlined in that plan.
- **Status Updates**: Update plan status only when a user requests use of `docs/plan/**`.

### Execution Plans vs. TodoWrite Tool

- **Execution Plans** (`docs/plan/**`): Multi-session, persistent task breakdown for cross-session coordination.
- **TodoWrite Tool**: Within-session progress tracking (ephemeral) for active implementation work.
- **Synchronization**: Keep execution plan status updated; TodoWrite supplements it during active work sessions.

## Missing Information & Self-Correction

### 1. Handling Missing Information

- Progress must not be stalled by minor, cosmetic, or non-material uncertainties.
- If logs, errors, or repository context are partial, the agent should proceed using **explicit assumptions**.
- Assumptions must be stated clearly before acting.
- Clarification is mandatory ONLY when missing information would materially affect the chosen architectural or strategic decision.

### 2. Self-Correction Rule (Mandatory)

- The agent MUST fix low-level mistakes it introduced (syntax errors, missing imports, formatting issues, obvious compile failures) immediately and without asking for permission.
- Only high-risk, irreversible, or wide-impact changes (e.g., deleting data, altering public API signatures to fix a bug) require confirmation before correction.

### 3. Out-of-Scope Issues

- If you discover bugs/issues outside your task scope during implementation:
  - Document them clearly in code comments or a separate note.
  - Do NOT fix them unless they block your current work.
  - Inform the user at task completion for triage.

## Git Workflow Rules

### Strict Read-Only Policy

Agents are **STRICTLY FORBIDDEN** from performing any git write operations. Only read-only inspection is allowed.

**✅ ALLOWED (Read-Only Operations):**
- `git status` - Check working tree status
- `git diff` - View changes (staged, unstaged, or between commits)
- `git log` - View commit history
- `git show` - Inspect specific commits
- `git branch` - List branches (view only)
- `git ls-files` - List tracked files

**❌ FORBIDDEN (All Write Operations):**
- `git add` - Staging files
- `git commit` - Creating commits
- `git push` - Pushing to remote
- `git pull` - Pulling from remote
- `git fetch` - Fetching from remote
- `git merge` - Merging branches
- `git rebase` - Rebasing commits
- `git reset` - Resetting HEAD or index
- `git checkout` - Switching branches or restoring files
- `git switch` - Switching branches
- `git restore` - Restoring files
- `git cherry-pick` - Cherry-picking commits
- `git stash` - Stashing changes
- `git tag` - Creating tags
- `git rm` - Removing files from git
- `git mv` - Moving files in git
- `git config` - Modifying git configuration
- Any other command that modifies repository state

### Rationale

- **User Control**: The user maintains full control over version control decisions.
- **Safety**: Prevents accidental commits, branch changes, or history rewrites.
- **Auditability**: All git operations are explicitly performed by the user.

### Validation

- **Tooling**: After any Rust code change, run `make ci-local` to validate.
- **Reporting**: If local validation isn't possible, mark as "Pending Verification" in the completion report.

## Multi-Agent Concurrency Rules

### Scope & Isolation

- Assume multiple agents are working concurrently; keep scope tight.
- **Scope Discipline**: Only modify files explicitly covered by your task.
- **Snapshot Awareness**: Record out-of-scope changes observed but don't modify them.
- **Foreign Changes**: If you detect unexpected diffs/files:
  - Do not modify or revert them.
  - Record them as "Out-of-Scope Changes Observed".
  - Ignore them for your task.

---

# Part III: Architecture & Standards

## Workspace & Layering

### Strict Responsibilities

- **`pavis-core`**: protocol + canonical semantics; canonical validation of `RuntimeConfig`; no I/O, parsing, or format concerns.
- **`pavis-codec-*`**: input DTOs, source-specific defaults/validation, transforms to `pavis-core::RuntimeConfig`.
- **`pavctl`**: I/O orchestration shell that invokes codecs.
- **`pavis-pvs`**: the only place to read/inspect `.pvs`, do magic/version/checksum checks, and run rkyv byte validation; **binary integrity only** (no semantic validation); runtime must not touch archive internals.
- **`pavis` runtime**: consumes **current-version** validated config; only defensive crash-safety checks; no parsing/serde/rkyv, no semantic validation or config decoding (normal runtime state allocation is fine); version mismatch is a hard error.
- **`pavis-relay`/`pavis-governor`**: control-plane migration and re-emission of current-version `.pvs` artifacts after core validation.

### Dependency Direction

- One-way flow: `pavis-core` is foundational; codecs/producers depend on core; runtime depends on core.
- Runtime MUST NOT depend on codecs/serde/rkyv.
- Shared domain types live in core.

## Modules & Structure

- Rust 2018+ layout: no `mod.rs`; use `<module>.rs` with submodules in `<module>/`.
- Keep `<module>.rs` focused on module structure and `pub use`; avoid business logic there.
- Split files by responsibility (data types vs business logic vs pvs/I/O) and to prevent circular deps—not by size alone.
- Extract shared, foundational data structs/enums into `types.rs`/`model.rs` or similar when used by multiple siblings; keep cohesive, local types in place to avoid import noise.
- Prefer minimal visibility (`pub(super)`, `pub(crate)`); do not widen for convenience.
- Preserve public APIs and crate boundaries; avoid new cross-layer dependencies. Keep diffs small and readable.

## E2E Test vs. Implementation Mismatch Protocol

**TL;DR:**
- Implementation bug (violates spec) → Fix code
- Test bug (wrong expectations) → Fix test
- Ambiguous behavior (no spec) → **ASK USER**

When e2e test expectations conflict with implementation logic:

### 1. Classification

- **Category A: Implementation Bug** - The implementation violates documented protocol/spec or has incorrect logic
  - *Action*: Fix the implementation to match test expectations
  - *Example*: Protocol spec requires 304 for unchanged configs, but code returns 200

- **Category B: Test Bug** - The test has incorrect expectations that don't match intended behavior
  - *Action*: Fix the test to match correct implementation
  - *Example*: Test expects auto-increment version to be 5 on first publish (should be 1)

- **Category C: Ambiguous/Undefined Behavior** - No clear spec or multiple valid interpretations
  - *Action*: **STOP and ask the user for clarification**
  - *Questions to ask*:
    1. What is the intended/expected behavior according to the protocol spec?
    2. Should the implementation be adjusted, or should the test be adjusted?
    3. Are there any external dependencies (clients, other services) that expect specific behavior?
  - *Example*: Long-poll should return immediately vs. wait when versions match

### 2. Decision Workflow

```
Mismatch Detected
    ↓
Is behavior documented in spec/protocol?
    ├─ Yes → Follow spec (Category A or B)
    └─ No → ASK USER (Category C)
        ↓
        User provides direction
        ↓
        Implement + document decision
```

### 3. Documentation Requirement

- Any Category C resolution MUST be documented in relevant files:
  - Protocol behavior → `ARCHITECTURE.md`
  - API contracts → Code comments + `README.md`
  - Test rationale → Test file comments

## Core Code & Cargo Modification Guard (Test Safety Rule)

- Tests MUST adapt to the architecture, not the other way around.
- Core crates (`pavis-core`, `pavis-pvs`, `pavis-*-api`) MUST NOT be modified solely to make tests easier.
- `Cargo.toml` MUST NOT be modified solely to add test or mocking dependencies.
- Exceptions are allowed only with explicit justification:
  - correctness, safety, or architectural necessity
  - boundary check against "Workspace & Layering" rules
  - alternatives considered

## General Rules

1. Read before writing—follow existing patterns.
2. Make minimal changes needed to solve the problem.
3. Use stable Rust only; avoid `#![feature(...)]`.
4. Respect manual edits—if a file changed since you last read it, preserve the user's updates.
5. **Documentation Updates (MANDATORY)**: After ANY code change, always consider and update relevant documentation:
   - **ARCHITECTURE.md**: For protocol changes, new components, or architectural decisions
   - **README.md**: For user-facing features, API changes, or usage instructions
   - **docs/project/roadmap.md**: For completed features or milestones (must refresh summary at top)
   - **docs/project/features.md**: For new features, status updates (✅/⏳/⚠️/❌), or explicitly dropped features
   - **Code comments**: For complex logic, rationale, or non-obvious implementation details
   - **Module docs (`//!`)**: When adding new modules or changing module responsibilities
   - **Function docs (`///`)**: For public APIs and complex functions
   - **Test documentation**: When e2e or benchmark test cases are added/changed/removed, document the rationale in test file comments
   - This is NOT optional—treat documentation as part of the implementation.
6. Backward compatibility is a lower concern (no public release yet) unless the user requests stability explicitly.
7. Do not create a new crate unless the user explicitly asks.
8. **Do not add/remove fields or change the structure of `RuntimeConfig`** unless explicitly instructed (internal refactoring is OK).

---

# Part IV: Code Quality & Style

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

## Tooling & Validation

- **After any Rust code change: run `make ci-local`.**

## Benchmarking

| Item        | Value                                |
| ----------- | ------------------------------------ |
| Location    | `bench/`                             |
| Command     | `make benchmark`                     |
| CI Workflow | `.github/workflows/bench.yaml`       |
| Reference   | [bench/README.md](../bench/README.md) |

---

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

### 9. Documentation & Roadmap (MANDATORY)
- [ ] **Project Docs**: Updated `ARCHITECTURE.md` for protocol/architectural changes?
- [ ] **User Docs**: Updated `README.md` for user-facing features or API changes?
- [ ] **Roadmap**: Updated `docs/project/roadmap.md` for completed milestones (with summary refresh)?
- [ ] **Features**: Updated `docs/project/features.md` for new features, status changes (✅/⏳/⚠️/❌), or dropped features?
- [ ] **Code Docs**: Added/updated `///` docs for public functions and structs?
- [ ] **Module Docs**: Added/updated `//!` docs for new or modified modules?
- [ ] **Inline Comments**: Added comments explaining "why" for complex logic?
- [ ] **Test Docs**: E2E and benchmark test file comments explain rationale when tests are added/changed/removed?
- [ ] **Completeness**: Documentation treated as part of implementation, not an afterthought?

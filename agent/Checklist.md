# Code Change & Readability Checklist

Derived from Audit Tasks 1–11 and Rust Readability Standards. Verify before completion.

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
- [ ] **Audit Logs**: Relevant reports in `audit/report/` updated with UTC timestamp?
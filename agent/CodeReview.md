# Code Review (Task 12)

## Navigation

- [Core Instructions](./AGENT.md)
- [Tasks](./Tasks.md)
- [Workflow](./Workflow.md)
- [Audit Overview](./AuditOverview.md)
- [Multi-Agent Rules](./MultiAgentRules.md)

## Task 12: Code Review (Uncommitted Changes or Last Commit)

- Review only the current change set, not the entire repository.
- The review target is:
  - Uncommitted changes in the working tree, OR
  - The most recent commit (e.g. `HEAD`), depending on what is present.

---

## Severity Classification (Mandatory)

Every finding MUST be classified using these severity levels:

| Severity | Label | Criteria | Action Required |
|----------|-------|----------|-----------------|
| 🚫 **Blocker** | `[BLOCKER]` | Breaks build, causes data loss, security vulnerability, or violates safety invariants | MUST fix before merge |
| 🔥 **Critical** | `[CRITICAL]` | Incorrect behavior, architectural violation, or regression risk | MUST fix before merge |
| ⚠️ **Warning** | `[WARNING]` | Potential bug, edge case not handled, or code smell | SHOULD fix before merge |
| 💡 **Suggestion** | `[SUGGESTION]` | Improvement opportunity, readability, or minor optimization | MAY fix (optional) |
| 📝 **Nit** | `[NIT]` | Style, naming, formatting preference | MAY fix (optional) |

---

## Review Scope (Strict Checklist)

### Mandatory Checks (MUST review for every change)

1. **Correctness**
   - Logic errors, off-by-one, null/None handling
   - Async safety (race conditions, deadlocks, missing awaits)
   - Resource leaks (file handles, connections, memory)

2. **Architectural Boundaries** (see `AGENT.md` "Workspace & Layering")
   - `pavis-core`: No I/O, no parsing, no format concerns
   - `pavis-pvs`: Binary integrity only, no semantic validation
   - `pavis` runtime: No codec/relay/ingest dependencies
   - `pavis-relay`: No DTO decoding in distribution path
   - Dependency direction violations

3. **API Design**
   - Visibility: Prefer `pub(crate)` over `pub` unless intentional
   - Breaking changes to public types
   - `unsafe` usage must have safety documentation

4. **Error Handling**
   - Unwraps/expects in non-test code (use `?` or explicit handling)
   - Error types match layer (anyhow for binaries, thiserror for libraries)
   - Panics in library code

5. **Type Safety**
   - `RuntimeConfig` struct changes (requires explicit approval)
   - Validation bypass paths
   - Serialization/deserialization correctness

### Secondary Checks (SHOULD review)

6. **Performance**
   - Allocations in hot paths
   - Unnecessary cloning
   - Blocking calls in async contexts

7. **Test Quality**
   - New code has corresponding tests
   - Tests verify behavior, not implementation
   - No test-only modifications to production code

8. **Documentation**
   - Public API doc comments present
   - Safety docs for unsafe code
   - No stale/misleading comments

### Style Checks (MAY review)

9. **Formatting & Style**
   - Follows rustfmt conventions
   - Consistent naming patterns
   - Idiomatic Rust

---

## Review Method

### Step 1: Identify Changes
```bash
git diff --stat                    # Overview
git diff                           # Uncommitted changes
git show HEAD                      # Last commit
git diff HEAD~1..HEAD              # Last commit diff
```

### Step 2: Categorize Files
- **Core changes**: `pavis-core/*` — highest scrutiny
- **Protocol changes**: `pavis-pvs/*` — binary format integrity
- **Runtime changes**: `pavis/*` — performance and safety focus
- **Relay changes**: `pavis-relay/*` — boundary and state management
- **Test changes**: `**/tests/*`, `pavis-e2e/*` — verify test quality
- **Config/CI changes**: `Cargo.toml`, `.github/*` — dependency review

### Step 3: Apply Severity-Based Review
- Scan for blockers/critical issues first
- Then warnings
- Finally suggestions/nits

---

## Output Format (Structured Review)

### Per-File Format
```markdown
## `path/to/file.rs`

### [SEVERITY] Short Title (L##-L##)

**Changed:** Description of what changed
**Issue:** Why this is problematic (or "Looks good")
**Suggestion:** Concrete fix or improvement
**Verification:** Commands to validate (if applicable)
```

### Summary Format
```markdown
## Review Summary

| Severity | Count | Files |
|----------|-------|-------|
| 🚫 Blocker | N | file1.rs, file2.rs |
| 🔥 Critical | N | ... |
| ⚠️ Warning | N | ... |
| 💡 Suggestion | N | ... |
| 📝 Nit | N | ... |

**Verdict:** APPROVE / REQUEST_CHANGES / COMMENT
**Blocking Issues:** List of must-fix items (if any)
```

---

## Blocking Rules (Hard Requirements)

The following MUST block approval:

1. **Build Failures**: Code that won't compile
2. **Test Failures**: Changes that break existing tests
3. **Security Issues**: Credential exposure, unsafe without docs, unbounded input
4. **Architectural Violations**: Wrong layer dependencies, boundary breaches
5. **Data Loss Risk**: Unvalidated writes, missing error handling on I/O
6. **`RuntimeConfig` Changes**: Without explicit user approval
7. **`Cargo.toml` Modifications**: Adding deps without justification
8. **Formatting Corruption**: Files with incorrect whitespace/encoding

---

## Boundary & Safety Checks (Expanded)

### Core Crate Guard
Changes to `pavis-core` MUST be reviewed for:
- No I/O operations added
- No new external dependencies
- `RuntimeConfig` struct unchanged (or explicitly approved)
- Validation logic remains in `validate.rs`
- No runtime-only state in domain types

### Protocol Crate Guard
Changes to `pavis-pvs` MUST be reviewed for:
- No semantic validation added (integrity only)
- Header format unchanged (or version bumped)
- Checksum algorithm unchanged
- rkyv serialization compatibility

### Runtime Crate Guard
Changes to `pavis` MUST be reviewed for:
- No codec/relay/ingest dependencies added
- No parsing/serde in hot paths
- `unsafe` blocks have safety documentation
- `ValidatedRuntimeConfig::from_trusted` usage is justified

### Test Safety Guard
Test changes MUST NOT:
- Modify production code solely for testability
- Add test-only dependencies to production `Cargo.toml`
- Weaken assertions to make tests pass
- Skip validation that production code requires

---

## Execution Constraints

- Do NOT run fmt, lint, tests, or benchmarks unless explicitly instructed.
- If validation is recommended, mark as **"Pending Verification"** with commands:
  ```
  Pending Verification:
  - `make fmt` — Check formatting
  - `make lint` — Run clippy
  - `make test` — Run unit tests
  - `make e2e-integrated` — Run E2E tests
  ```

---

## Review Checklist (Quick Reference)

Before approving, verify:

- [ ] No blockers or critical issues remain
- [ ] Architectural boundaries respected
- [ ] Error handling is appropriate
- [ ] No unwraps in library code
- [ ] Public API changes are intentional
- [ ] Tests cover new behavior
- [ ] No formatting corruption
- [ ] `RuntimeConfig` unchanged (or approved)
- [ ] `Cargo.toml` changes justified
- [ ] Documentation updated if needed

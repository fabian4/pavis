# Code Review Protocol (Task 12)

## Navigation

- [Core Instructions](./README.md)
- [Audit Protocol](./AUDIT.md)
- [Operations Manual](./OPERATIONS.md)

## Task 12: Code Review

- **Target**: Uncommitted changes OR the most recent commit (`HEAD`).
- **Scope**: Current change set only.

---

## Severity Classification

| Label | Criteria | Action |
|-------|----------|--------|
| `[BLOCKER]` | Breaks build, safety, or security. | **MUST** fix. |
| `[CRITICAL]` | Bug, regression, or arch violation. | **MUST** fix. |
| `[WARNING]` | Edge case, code smell. | **SHOULD** fix. |
| `[SUGGESTION]` | Improvement, minor opt. | Optional. |
| `[NIT]` | Style, naming. | Optional. |

---

## Review Scope Checklist

### 1. Mandatory Checks
- **Correctness**: Logic errors, async safety, resource leaks.
- **Boundaries**:
  - `pavis-core`: No I/O.
  - `pavis-pvs`: Integrity only.
  - `pavis-relay`: No DTO decoding.
- **API Design**: Minimal visibility, documented `unsafe`.
- **Error Handling**: No unwraps in lib code.
- **Type Safety**: `RuntimeConfig` changes require explicit approval.

### 2. Secondary Checks
- **Performance**: Allocations in hot paths.
- **Tests**: New code covered? Tests verify behavior?
- **Docs**: Public APIs documented.

---

## Output Format

### Per-File
```markdown
## `path/to/file.rs`
### [SEVERITY] Short Title (L##-L##)
**Changed:** ...
**Issue:** ...
**Suggestion:** ...
```

### Summary
```markdown
## Review Summary
| Severity | Count | Files |
|----------|-------|-------|
| ... | ... | ... |

**Verdict:** APPROVE / REQUEST_CHANGES
```

---

## Blocking Rules
1. Build/Test failures.
2. Security issues (secrets, unsafe).
3. Architectural violations.
4. `RuntimeConfig` changes without approval.
5. Formatting/Encoding corruption.

## Boundary Guards
- **Core**: No I/O, no new deps.
- **Protocol**: No semantic validation.
- **Runtime**: No codec/relay deps.
- **Tests**: No modifying production code for testing.

## Execution
- Do **NOT** run fmt/test unless instructed.
- Mark verification as **"Pending Verification"** if needed.

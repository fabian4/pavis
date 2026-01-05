# Audit Protocol & Tasks

## Navigation

- [Core Instructions](./README.md)
- [Operations Manual](./OPERATIONS.md)
- [Code Review](./REVIEW.md)

---

## 1. Audit Reporting Rules

### Report Format
- Reports live in `../audit/report/`.
- **Do not restructure** existing reports; only append new entries.
- **Traceability**: Every entry must include the Model Identifier and UTC Timestamp.
- **Prioritization**: Update the "Open Findings" table in each report.

### README Generation
- `../audit/README.md` is the single source of truth.
- Regenerate it using:
  - Open findings from `../audit/report/*.md`.
  - Roadmap summary from `../ROADMAP.md`.
  - Coverage summary from `../audit/coverage.md`.

### Allowed Writes (Scope Fence)
- **Tasks 1, 4, 6-11**: Modify `../audit/report/<TASK_REPORT>.md` and `../audit/README.md`.
- **Task 5**: Also read `../audit/coverage.md`.
- **Tasks 2-3**: May also update `../ROADMAP.md`.

---

## 2. Task Definitions (1-11)

### Severity Classification
| Level | Label | Criteria |
|-------|-------|----------|
| 🚫 | Blocker | Architectural violation, safety issue, or blocking defect. |
| 🔥 | High | Significant issue requiring attention before next phase. |
| ⚠️ | Medium | Notable concern that should be addressed. |
| 🧹 | Low | Minor issue or improvement opportunity. |

---

### Task 1: Architecture Compliance
**Objective**: Verify `ARCHITECTURE.md` compliance.
**Strict Checks**:
1. **Layer Boundaries**: `pavis-core` (Semantic) -> `pavis-pvs` (Integrity) -> `pavis` (Runtime).
2. **Dependencies**: No reverse dependencies (e.g., core depending on runtime).
3. **Contracts**: HTTP API, PVS Header, and Validation strategy match docs.
**Output**: `../audit/report/ARCH_COMPLIANCE.md`

### Task 2: Architecture vs Roadmap
**Objective**: Ensure planned features align with architectural constraints.
**Strict Checks**:
1. Roadmap items map to defined components.
2. Phasing respects dependency order.
**Output**: `../audit/report/ARCH_ROADMAP_ALIGNMENT.md`

### Task 3: Roadmap vs Implementation
**Objective**: Verify `ROADMAP.md` reflects reality.
**Strict Checks**:
1. Evidence exists for `[x]` items.
2. No code exists for `[ ]` items.
3. Untracked features are flagged.
**Output**: `../audit/report/ROADMAP_REVIEW.md` (Update roadmap checkboxes).

### Task 4: Code Structure
**Objective**: Enforce Rust file/module organization.
**Thresholds**:
- Production files > 600 lines: **MUST** split.
- Test files > 800 lines: Review.
- No `mod.rs` files (Use `module.rs` + `module/`).
**Output**: `../audit/report/STRUCTURE_REVIEW.md`

### Task 5: Test Coverage
**Objective**: Verify test quality and coverage.
**Matrix**:
- Core/PVS: Unit tests required.
- Codec/Relay: Unit + Integration required.
- Runtime: E2E required.
**Output**: `../audit/report/TEST_COVERAGE_REVIEW.md` (Read `../audit/coverage.md`).

### Task 6: Public API & Stability
**Objective**: Minimize public surface area.
**Checks**:
- Default visibility `pub(crate)`.
- All `pub` items documented.
- No leaking of internal types.
**Output**: `../audit/report/PUBLIC_API_REVIEW.md`

### Task 7: Comments
**Objective**: Quality over quantity.
**Checks**:
- `// Safety:` comments for all `unsafe` blocks.
- Doc comments (`///`) for public API.
- No commented-out code.
**Output**: `../audit/report/COMMENT_REVIEW.md`

### Task 8: Duplication
**Objective**: Identify redundant logic/tests.
**Thresholds**:
- Logic repeated 3+ times: MUST consolidate.
- Test fixtures repeated 2+ times: SHOULD share.
**Output**: `../audit/report/DUPLICATION_REVIEW.md`

### Task 9: Security
**Objective**: Risk assessment.
**Checks**:
- Secrets/Credentials in code.
- Unsafe code justification.
- Dependency vulnerabilities (audit).
**Output**: `../audit/report/SECURITY_REVIEW.md`

### Task 10: Dependency Boundaries
**Objective**: Enforce crate graph hygiene.
**Checks**:
- `pavis-core`: No I/O deps.
- `pavis-relay`: No DTO decoding logic.
- No lateral deps between codecs.
**Output**: `../audit/report/DEPENDENCY_BOUNDARY_REVIEW.md`

### Task 11: Performance
**Objective**: Hot path analysis.
**Checks**:
- Zero allocations in routing path.
- No blocking calls in async contexts.
- Lock-free config lookups.
**Output**: `../audit/report/PERFORMANCE_REVIEW.md`

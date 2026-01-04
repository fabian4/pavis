# Tasks (1-11)

## Navigation

- [Core Instructions](./AGENT.md)
- [Workflow](./Workflow.md)
- [Audit Overview](./AuditOverview.md)
- [Multi-Agent Rules](./MultiAgentRules.md)
- [Code Review](./CodeReview.md)

See [Workflow.md](./Workflow.md) for execution steps and [AuditOverview.md](./AuditOverview.md) for report rules.

---

## Severity Classification (All Tasks)

All findings across tasks 1-11 MUST use consistent severity levels:

| Severity | Label | Criteria |
|----------|-------|----------|
| 🚫 **Blocker** | Blocker | Architectural violation, safety issue, or blocking defect |
| 🔥 **High** | High | Significant issue requiring attention before next phase |
| ⚠️ **Medium** | Medium | Notable concern that should be addressed |
| 🧹 **Low** | Low | Minor issue or improvement opportunity |

---

## Task 1: Architecture Compliance Review

### Objective
Verify the entire repository implements `ARCHITECTURE.md` correctly.

### Strict Requirements

1. **Layer Boundary Verification** (MUST check all):

   | Crate | MUST | MUST NOT |
   |-------|------|----------|
   | `pavis-core` | Define canonical types, semantic validation | Have I/O, parsing, format concerns, external deps |
   | `pavis-pvs` | Binary integrity (magic, version, checksum) | Perform semantic validation, decode RuntimeConfig |
   | `pavis` runtime | Depend only on core + pvs | Depend on codecs, relay, ingest, serde |
   | `pavis-relay` | Distribute PVS artifacts, manage versions | Parse DTOs in distribution path, decode RuntimeConfig |
   | `pavis-codec-*` | Transform DTO → RuntimeConfig, call core validation | Perform I/O, networking, governance |

2. **Dependency Direction Audit**:
   - Verify `Cargo.toml` dependencies follow architecture graph
   - Flag any reverse dependencies (e.g., core depending on runtime)
   - Check for transitive violations through re-exports

3. **Responsibility Boundary Audit**:
   - Verify each crate's code matches its documented responsibility
   - Flag code that belongs in a different layer
   - Check for "boundary creep" (layer doing adjacent layer's work)

4. **Contract Verification**:
   - HTTP API endpoints match Architecture Sec 3.1
   - PVS header format matches Architecture Sec 6.1
   - Validation strategy matches Architecture Sec 7.1

### Classification Rules
For each inconsistency, classify as:
- **Implementation Bug**: Code violates architecture intent
- **Documentation Drift**: ARCHITECTURE.md outdated vs implementation
- **Acceptable Deviation**: Justified exception (must document reason)

### Output
- Report: `../audit/report/ARCH_COMPLIANCE.md`
- MUST NOT modify `ARCHITECTURE.md` in this task

---

## Task 2: Architecture vs Roadmap Consistency Review

### Objective
Ensure `ROADMAP.md` aligns with `ARCHITECTURE.md` constraints and phasing.

### Strict Requirements

1. **Constraint Alignment**:
   - Every roadmap item must respect architectural boundaries
   - Planned features must not violate layer responsibilities
   - Phase sequencing must match architecture dependencies

2. **Component Mapping**:
   - Each roadmap component maps to an architecture-defined crate
   - No roadmap items assume undefined components
   - Governor/Operator role clarity

3. **Gap Analysis**:
   - Architecture responsibilities tracked in roadmap
   - No orphaned architectural requirements
   - Phase dependencies correctly ordered

### Classification Rules
- **Conflict**: Roadmap item violates architecture
- **Missing**: Architecture requirement not in roadmap
- **Misaligned**: Phase ordering conflicts with dependencies

### Output
- Report: `../audit/report/ARCH_ROADMAP_ALIGNMENT.md`
- MAY update `ROADMAP.md` to fix alignment issues
- MUST NOT modify `ARCHITECTURE.md`

---

## Task 3: Roadmap vs Implementation Review

### Objective
Verify `ROADMAP.md` accurately reflects implementation status.

### Strict Requirements

1. **Status Verification Matrix**:

   | Roadmap Status | Expected Evidence |
   |----------------|-------------------|
   | `[x]` Complete | Working code + tests at specified paths |
   | `[ ]` Planned | No implementation present |
   | In Progress | Partial implementation identifiable |

2. **Evidence Collection**:
   - For each roadmap item, locate implementing code
   - Document file paths and function names
   - Note any partial implementations

3. **Discrepancy Types**:
   - **Implemented but unchecked**: Code exists, roadmap says planned
   - **Checked but missing**: Roadmap says done, no evidence
   - **Untracked feature**: Code exists, not in roadmap
   - **Stale description**: Implementation differs from roadmap text

### Output
- Report: `../audit/report/ROADMAP_REVIEW.md`
- MUST update `ROADMAP.md` checkboxes to match reality
- MUST update roadmap summary section after changes

---

## Task 4: Rust Code Structure & File Size Review

### Objective
Ensure code organization follows Rust best practices and single-responsibility principle.

### Strict Requirements

1. **File Size Thresholds**:

   | Category | Warning | Action Required |
   |----------|---------|-----------------|
   | Production code | >400 lines | Review for split |
   | Production code | >600 lines | MUST propose split |
   | Test code | >800 lines | Review for split |
   | Config builders | No limit | Exception allowed |

2. **Module Organization Rules**:
   - No `mod.rs` files (Rust 2018+ layout)
   - `<module>.rs` + `<module>/` for submodules
   - Module files focus on structure, not business logic
   - Single responsibility per module

3. **Cohesion Checks**:
   - Each file has one clear purpose
   - Related types grouped together
   - No unrelated features in same file
   - Shared utilities extracted appropriately

4. **Test Placement**:
   - Unit tests colocated in `#[cfg(test)]` modules
   - Integration tests in `tests/` directory
   - No test helpers in production code paths

### Output
- Report: `../audit/report/STRUCTURE_REVIEW.md`
- Analysis only: do not refactor unless requested

---

## Task 5: Test Coverage & Quality Review

### Objective
Ensure comprehensive, high-quality test coverage across all test types.

### Strict Requirements

1. **Coverage Thresholds** (informational, not blocking):

   | Category | Target | Critical Minimum |
   |----------|--------|------------------|
   | Core validation | 95%+ | 80% |
   | Protocol (pvs) | 90%+ | 75% |
   | Runtime hot paths | 85%+ | 70% |
   | Relay handlers | 85%+ | 70% |
   | Startup/main | 50%+ | Not blocking |

2. **Test Category Matrix**:

   | Feature | Unit | Integration | E2E |
   |---------|:----:|:-----------:|:---:|
   | Core validation | ✓ Required | — | — |
   | PVS integrity | ✓ Required | — | — |
   | Codec transforms | ✓ Required | ✓ Required | — |
   | Relay HTTP API | ✓ Required | ✓ Required | ✓ Required |
   | Runtime routing | ✓ Required | — | ✓ Required |
   | Hot reload | — | — | ✓ Required |

3. **E2E Case Verification**:
   - Cross-reference `CASES_*.md` against test implementations
   - Verify each planned case has corresponding test file
   - Flag missing or incomplete E2E scenarios

4. **Test Quality Criteria**:
   - Assertions test behavior, not implementation
   - Error paths explicitly tested
   - No `#[ignore]` without documented reason
   - Deterministic (no flaky tests)

5. **CI Workflow Review**:
   - Required jobs: fmt, clippy, test, e2e
   - Caching configured for cargo
   - Appropriate triggers (PR, push, schedule)

### Output
- Report: `../audit/report/TEST_COVERAGE_REVIEW.md`
- Coverage data from `../audit/coverage.md`
- Open findings for critical gaps

---

## Task 6: Public API & Boundary Stability Review

### Objective
Ensure public APIs are minimal, intentional, and stable.

### Strict Requirements

1. **Visibility Audit**:

   | Crate Type | Default Visibility | Exceptions |
   |------------|-------------------|------------|
   | Library (core, pvs, apis) | `pub(crate)` | Documented public API |
   | Binary (pavis, relay, pavctl) | `pub(crate)` | Module re-exports only |

2. **Public API Checklist**:
   - [ ] Every `pub` item is intentionally public
   - [ ] Public types have doc comments
   - [ ] No internal types leaked through public signatures
   - [ ] `unsafe` functions have safety documentation

3. **Boundary Type Analysis**:
   - Identify types that cross crate boundaries
   - Verify they belong in the exporting crate
   - Flag types that should be facades/wrappers

4. **Breaking Change Detection**:
   - Flag any changes to existing public signatures
   - Note additions to public API surface
   - Identify removals or deprecations

### Output
- Report: `../audit/report/PUBLIC_API_REVIEW.md`

---

## Task 7: Code Comment Quality Review

### Objective
Ensure comments are accurate, useful, and not redundant.

### Strict Requirements

1. **Comment Categories**:

   | Type | Requirement |
   |------|-------------|
   | `///` Doc comments | Required for all public items |
   | `//!` Module docs | Required for public modules |
   | `// Safety:` | Required before `unsafe` blocks |
   | `// TODO:` | Must have issue reference or timeline |
   | `// FIXME:` | Must be tracked or removed |
   | Inline comments | Only for non-obvious logic |

2. **Quality Criteria**:
   - Grammatically correct
   - Technically accurate (matches code behavior)
   - Not redundant (doesn't restate obvious code)
   - Not stale (matches current implementation)

3. **Anti-Patterns to Flag**:
   - Comments describing what code does (vs why)
   - Commented-out code blocks
   - References to removed files/functions
   - Outdated behavior descriptions

### Output
- Report: `../audit/report/COMMENT_REVIEW.md`
- Include UTC timestamp in each entry

---

## Task 8: Duplication & Redundancy Review

### Objective
Identify and consolidate duplicated code, tests, docs, and CI.

### Strict Requirements

1. **Duplication Categories**:

   | Category | Threshold | Action |
   |----------|-----------|--------|
   | Code (same logic) | 3+ occurrences | MUST consolidate |
   | Code (similar logic) | 5+ occurrences | SHOULD consolidate |
   | Test fixtures | 2+ occurrences | SHOULD share |
   | CI job steps | 3+ occurrences | SHOULD extract |

2. **Detection Scope**:
   - Function bodies with identical/similar logic
   - Struct definitions with overlapping fields
   - Test setup/teardown patterns
   - Error handling boilerplate
   - CI workflow job definitions

3. **Consolidation Strategies**:
   - Shared utility modules
   - Trait abstractions
   - Macros (only if cleaner than functions)
   - Test harness helpers
   - CI reusable workflows

### Output
- Report: `../audit/report/DUPLICATION_REVIEW.md`

---

## Task 9: Security Review

### Objective
Identify security risks in dependencies, unsafe code, and secrets handling.

### Strict Requirements

1. **Unsafe Code Audit**:

   | Check | Requirement |
   |-------|-------------|
   | `unsafe` blocks | Must have `// Safety:` comment |
   | `unsafe fn` | Must have `# Safety` doc section |
   | FFI boundaries | Must validate all inputs |
   | Transmute/pointer casts | Must justify memory layout |

2. **Dependency Security**:
   - Check for known vulnerabilities (cargo-audit output if available)
   - Flag unmaintained dependencies (>1 year no updates)
   - Note dependencies with `unsafe` in public API

3. **Secret Scanning**:
   - No hardcoded credentials
   - No API keys in source
   - No private keys in test fixtures
   - Environment variable usage for secrets

4. **Input Validation**:
   - All external input validated
   - Size limits on buffers
   - Timeout limits on operations

### Output
- Report: `../audit/report/SECURITY_REVIEW.md`

---

## Task 10: Dependency Boundary Review

### Objective
Verify crate dependencies follow architectural layering rules.

### Strict Requirements

1. **Layer Dependency Matrix**:

   | Crate | Allowed Dependencies |
   |-------|---------------------|
   | `pavis-core` | rkyv, thiserror, regex (no I/O crates) |
   | `pavis-pvs` | pavis-core, rkyv, sha2 |
   | `pavis` | pavis-core, pavis-pvs, pingora |
   | `pavis-relay` | pavis-pvs, pavis-ingest-*, pavis-codec-*, axum |
   | `pavis-codec-*` | pavis-core, pavis-codec-api, serde |
   | `pavis-ingest-*` | pavis-ingest-api, tokio |

2. **Violation Types**:
   - **Reverse dependency**: Lower layer depends on higher
   - **Lateral dependency**: Peer crates depending on each other
   - **Transitive violation**: Indirect dependency through re-export
   - **Dev-dep leakage**: Test dependency in production path

3. **Feature Flag Review**:
   - Optional features properly gated
   - No unconditional heavy dependencies
   - serde feature optional where appropriate

### Output
- Report: `../audit/report/DEPENDENCY_BOUNDARY_REVIEW.md`

---

## Task 11: Performance & Allocation Review

### Objective
Identify performance bottlenecks and excessive allocations.

### Strict Requirements

1. **Hot Path Identification**:

   | Path | Performance Target |
   |------|-------------------|
   | Request routing | Zero allocations |
   | Config lookup | Lock-free reads |
   | Load balancing | Atomic operations only |
   | Access logging | Non-blocking |

2. **Allocation Analysis**:
   - Flag `clone()` in hot paths
   - Flag `String`/`Vec` creation in request path
   - Flag `Box::new` in frequently called code
   - Verify `Arc` usage is justified

3. **Async Analysis**:
   - No blocking calls in async contexts
   - No `block_on` inside async functions
   - Appropriate use of `spawn_blocking`

4. **Startup Analysis**:
   - Config loading efficiency
   - Regex pre-compilation
   - Connection pool initialization

5. **Classification**:

   | Type | Criteria |
   |------|----------|
   | Bug | Correctness issue (memory leak, deadlock) |
   | Scalability Risk | Will degrade under load |
   | Optimization | Improvement opportunity |

### Output
- Report: `../audit/report/PERFORMANCE_REVIEW.md`

---

## Report Quality Standards (All Tasks)

### Required Sections
1. **Summary** with severity counts
2. **Open Findings** (prioritized table)
3. **Review Entry** with:
   - Scope
   - Method
   - Model identifier
   - UTC timestamp
   - Findings with evidence

### Evidence Requirements
- File paths for code references
- Line numbers where applicable
- Quotes for documentation issues
- Commands for reproduction

### Status Tracking
- Open findings in prioritized table
- Resolved findings moved to history
- No duplicate entries across review runs

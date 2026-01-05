# Pavis Audit Dashboard

This dashboard provides a top-level summary of the codebase quality, architectural compliance, and roadmap progress.

---

## 📊 Status Overview

| Metric | Status |
|:---|:---|
| **Architectural Integrity** | ✅ Compliant (Minor safety note) |
| **Test Coverage** | ✅ 96.17% |
| **Roadmap Progress** | 🚧 Phase 3 (Dynamic Config) |
| **Open Findings** | 1 High, 2 Medium, 6 Low |

---

## 🚀 Roadmap Summary
*Extracted from [ROADMAP.md](../ROADMAP.md)*

- **Total Progress**: 17/59 (28%)
- **Core Features**: 17/43 (39%)
- **Technical Debt**: 0/16 (0%)

**Current Focus**: Phase 3 (Dynamic Configuration) & Phase 9 (xDS)

---

## 🛑 Open Findings (Prioritized)
*Aggregated from [audit/report/*.md](./report/)*

| ID | Severity | Report | Short Title |
|:---|:---:|:---|:---|
| F-2 | 🔥 High | [Performance](./report/PERFORMANCE_REVIEW.md) | Unnecessary path allocation in proxy hot path |
| F-3 | ⚠️ Medium | [Performance](./report/PERFORMANCE_REVIEW.md) | Access log formatting on request path |
| F-4 | ⚠️ Medium | [Performance](./report/PERFORMANCE_REVIEW.md) | O(N) linear scan for VirtualHost matching |
| F-1 | 🧹 Low | [Performance](./report/PERFORMANCE_REVIEW.md) | PVS loading reads entire file into heap |
| F-5 | 🧹 Low | [Performance](./report/PERFORMANCE_REVIEW.md) | Synchronous I/O in AccessLogWorker startup |
| F-1 | 🧹 Low | [Arch Compliance](./report/ARCH_COMPLIANCE.md) | Runtime relies on `unsafe from_trusted` for config loading |
| F-1 | 🧹 Low | [Structure](./report/STRUCTURE_REVIEW.md) | Legacy `mod.rs` usage in E2E tests |
| F-2 | 🧹 Low | [Structure](./report/STRUCTURE_REVIEW.md) | `worker.rs` approaching split threshold |
| F-1 | 🧹 Low | [Duplication](./report/DUPLICATION_REVIEW.md) | `minimal_config` boilerplate duplicated across crates |

---

## 🧪 Test Coverage Summary
*Extracted from [audit/coverage.md](./coverage.md)*

**Overall Coverage: 96.17%**

| Component | Coverage |
|:---|:---:|
| `pavis-core` | 98.4% |
| `pavis-pvs` | 100.0% |
| `pavis` runtime | 92.5% |
| `pavis-relay` | 90.2% |
| `pavctl` | 98.1% |

---

## 📂 Audit Reports

- [Architecture Compliance](./report/ARCH_COMPLIANCE.md)
- [Architecture vs Roadmap](./report/ARCH_ROADMAP_ALIGNMENT.md)
- [Roadmap Review](./report/ROADMAP_REVIEW.md)
- [Structure & Organization](./report/STRUCTURE_REVIEW.md)
- [Test Coverage & Quality](./report/TEST_COVERAGE_REVIEW.md)
- [Public API Stability](./report/PUBLIC_API_REVIEW.md)
- [Security Review](./report/SECURITY_REVIEW.md)
- [Dependency Boundaries](./report/DEPENDENCY_BOUNDARY_REVIEW.md)
- [Performance & Allocations](./report/PERFORMANCE_REVIEW.md)
- [Code Comment Quality](./report/COMMENT_REVIEW.md)
- [Duplication & Redundancy](./report/DUPLICATION_REVIEW.md)

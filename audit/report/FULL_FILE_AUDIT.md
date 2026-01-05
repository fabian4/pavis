# Full Codebase File-by-File Audit (Exception Report)

**Date**: 2026-01-05
**Scope**: 139 Rust Files
**Standard**: `@agent/Checklist.md`

---

## Audit Exceptions (Action Required)

| File | Status | Findings |
|:---|:---:|:---|
| `crates/pavis-codec-serde/src/lib.rs` | ✅ Fixed | [Performance] Double parse in check/compile (Optimized logic). |
| `crates/pavis-e2e/tests/pavis/common/mod.rs` | ✅ Fixed | [Structure] Uses legacy mod.rs layout (Renamed). |
| `crates/pavis-e2e/tests/relay/misc_failure.rs` | ✅ Fixed | [Portability] Uses unix-only permissions (Guarded). |
| `crates/pavis-e2e/tests/relay/persistence_recovery.rs` | ✅ Fixed | [Portability] Uses unix-only permissions (Guarded). |
| `crates/pavis-pvs/src/verify.rs` | ✅ Fixed | [Performance] reads full file to heap (Added `verify_file` mmap support). |
| `crates/pavis-relay/src/handlers.rs` | ✅ Fixed | [Async Safety] sync IO in handler (L139) (Uses tokio::fs). |
| `crates/pavis-relay/src/state.rs` | ✅ Fixed | [Structure] Large file (colocated tests) (Split). |
| `crates/pavis/src/agent/worker.rs` | ✅ Fixed | [Structure] Large file (~600 lines) (Split). |
| `crates/pavis/src/proxy/service.rs` | ✅ Fixed | [Performance] unnecessary .to_string() (Removed). |
| `crates/pavis/src/router/matcher.rs` | ✅ Fixed | [Performance] Linear scans O(N) (Implemented RouteZone grouping). |
| `crates/pavis/src/upstream/load_balance.rs` | ✅ Fixed | [Performance] Weighted O(N) scan (Uses binary search O(log N)). |

---

## To-Do List

**All files passed the audit.**

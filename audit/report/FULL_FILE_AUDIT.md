# Full Codebase File-by-File Audit (Exception Report)

**Date**: 2026-01-05
**Scope**: 139 Rust Files
**Standard**: `@agent/Checklist.md`

---

## Audit Exceptions (Action Required)

| File | Status | Findings |
|:---|:---:|:---|
| `crates/pavis-codec-serde/src/lib.rs` | ⚠️ Warning | [Performance] Double parse in check/compile. |
| `crates/pavis-e2e/tests/pavis/common/mod.rs` | ⚠️ Warning | [Structure] Uses legacy mod.rs layout. |
| `crates/pavis-e2e/tests/relay/misc_failure.rs` | ⚠️ Warning | [Portability] Uses unix-only permissions. |
| `crates/pavis-e2e/tests/relay/persistence_recovery.rs` | ⚠️ Warning | [Portability] Uses unix-only permissions. |
| `crates/pavis-pvs/src/verify.rs` | ⚠️ Warning | [Performance] reads full file to heap. |
| `crates/pavis-relay/src/handlers.rs` | ⚠️ Warning | [Async Safety] sync IO in handler (L139). |
| `crates/pavis-relay/src/state.rs` | ⚠️ Warning | [Structure] Large file (colocated tests). |
| `crates/pavis/src/agent/worker.rs` | ⚠️ Warning | [Structure] Large file (~600 lines). |
| `crates/pavis/src/proxy/service.rs` | ⚠️ Warning | [Performance] unnecessary .to_string(). |
| `crates/pavis/src/router/matcher.rs` | ⚠️ Warning | [Performance] Linear scans O(N). |
| `crates/pavis/src/upstream/load_balance.rs` | ⚠️ Warning | [Performance] Weighted O(N) scan. |

---

## To-Do List

1.  **Refactor**: Optimize `pavis/src/proxy/service.rs` to use `&str` in `match_request` path lookup (Performance).
2.  **Refactor**: Use `tokio::fs` in `pavis-relay/src/handlers.rs` `post_publish` (Async Safety).
3.  **Refactor**: Split `pavis/src/agent/worker.rs` tests into a separate module (Structure).
4.  **Refactor**: Split `pavis-relay/src/state.rs` tests (Structure).
5.  **Refactor**: Rename `pavis-e2e/tests/pavis/common/mod.rs` to `common.rs` (Structure).
6.  **Fix**: Add `#[cfg(unix)]` guards or cross-platform logic to `misc_failure.rs` and `persistence_recovery.rs` (Portability).
7.  **Optimize**: Improve `pavis-pvs/src/verify.rs` to use memory mapping or streaming for verification (Performance).
8.  **Optimize**: Implement O(1) lookup in `router/matcher.rs` (Performance).
9.  **Optimize**: Remove double parsing in `pavis-codec-serde` (Performance).

**All other files passed the audit.**

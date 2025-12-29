# Roadmap vs Implementation Review

## Active Findings (Latest)

- None.

## Historical Reviews

### Review 2025-12-29T17:56:58Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: full workspace

Summary:
- Confirmed ROADMAP status updates now reflect the implemented Phase 2, Phase 3, and Phase 6 items.

Findings:
- [DONE] Phase 2 `pavis-pvs` test items are implemented but still marked pending
  - Evidence: `ROADMAP.md` now marks the `check_archived_root` regression tests and version/algorithm mismatch tests as complete.
  - Resolution: Roadmap updated to reflect implemented `pavis-pvs` tests.

- [DONE] Phase 3 relay response headers and config history differ from roadmap status/text
  - Evidence: `ROADMAP.md` now lists checksum headers as SHA-256 with algorithm label, and config history as unbounded.
  - Resolution: Roadmap entries updated to match relay behavior.

- [DONE] Phase 3 long-poll header override is implemented but still unchecked
  - Evidence: `ROADMAP.md` now marks `distribution.long_poll.headers.algorithm` as complete.
  - Resolution: Roadmap updated to reflect current relay configuration support.

- [DONE] Phase 6 TLS implementation is partially complete but remains fully unchecked
  - Evidence: `ROADMAP.md` now marks `TlsConfig` (cert/key), server-side TLS, client-side TLS, and TLS termination E2E test as complete.
  - Resolution: Roadmap updated to reflect current TLS implementation coverage.

Resolved:
- Phase 2 `pavis-pvs` test items are implemented but still marked pending.
- Phase 3 relay response headers and config history differ from roadmap status/text.
- Phase 3 long-poll header override is implemented but still unchecked.
- Phase 6 TLS implementation is partially complete but remains fully unchecked.

Notes:
- Timestamp (UTC): 2025-12-29T17:56:58Z
- Limitations: Review confirmed roadmap edits only; no new feature validation performed.

### Review 2025-12-29T17:42:57Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: full workspace

Summary:
- Multiple Phase 2 and Phase 3 items are already implemented and need roadmap status updates.
- TLS support in runtime is present but not reflected in Phase 6.

Findings (by phase/component):

Phase 2: Protocol
- [NEW] `pavis-pvs` corrupted-payload regression tests are implemented
  - Roadmap item: "`check_archived_root` regression tests for corrupted payloads" (unchecked).
  - Evidence: `crates/pavis-pvs/src/verify.rs` test `verify_rejects_truncated_archive_payload`.
  - Suggested status: mark complete.

- [NEW] `pavis-pvs` version/algorithm mismatch test coverage is implemented
  - Roadmap item: "Version mismatch/unsupported algorithm coverage in tests" (unchecked).
  - Evidence: `crates/pavis-pvs/src/verify.rs` tests `verify_bytes_rejects_version_mismatch` and `verify_bytes_rejects_unsupported_algorithm`.
  - Suggested status: mark complete.

Phase 3: Long Polling (`pavis-relay`)
- [NEW] Response checksum headers are implemented with SHA-256 + algorithm label
  - Roadmap item: `X-Pavis-Checksum` listed as xxhash; `X-Pavis-Checksum-Alg` missing.
  - Evidence: `crates/pavis-relay/src/handlers.rs` sets checksum and checksum-alg headers; `crates/pavis-relay/tests/relay_http.rs` asserts header presence.
  - Suggested status: mark checksum headers complete, update description to SHA-256, add algorithm header entry.

- [NEW] Config history is unbounded, not "last N"
  - Roadmap item: "Config history (last N versions)" marked complete.
  - Evidence: `crates/pavis-relay/src/state.rs` stores history in a `HashMap` with no pruning.
  - Suggested status: update roadmap text to match unbounded history or add pruning task.

- [NEW] Long-poll header override for algorithm is implemented
  - Roadmap item: "distribution.long_poll.headers.algorithm" unchecked.
  - Evidence: `crates/pavis-relay/src/main.rs` reads `headers.algorithm` into `RelayOptions`.
  - Suggested status: mark complete.

Phase 6: Security (`pavis-core`, `pavis`)
- [NEW] TLS configuration and runtime TLS are partially implemented
  - Roadmap items: TLS config and runtime TLS implementation remain unchecked.
  - Evidence: `crates/pavis-core/src/runtime/server.rs` defines `TlsConfig`; `crates/pavis/src/main.rs` enables server TLS; `crates/pavis/src/proxy/service.rs` configures upstream TLS; e2e coverage in `crates/pavis-e2e/tests/tls_support.rs` and `crates/pavis-e2e/tests/upstream_tls.rs`.
  - Suggested status: mark cert/key TLS config, server-side TLS, and client-side TLS origination as complete; leave mTLS and advanced config pending.

Resolved:
- None.

Notes:
- Timestamp (UTC): 2025-12-29T17:42:57Z
- Limitations: Focused on clear code-to-roadmap mismatches; deeper feature completeness may require additional review.

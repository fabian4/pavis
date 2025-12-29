# Architecture vs Roadmap Alignment Review

## Active Findings (Latest)

- None.

## Historical Reviews

### Review 2025-12-29T17:55:29Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Documents reviewed: `Architecture.md`, `ROADMAP.md`

Summary:
- Confirmed prior alignment issues were resolved via ROADMAP updates.

Findings:
- [DONE] Roadmap checksum algorithm conflicts with Architecture
  - Evidence: `ROADMAP.md` now lists `X-Pavis-Checksum` as sha256 and includes `X-Pavis-Checksum-Alg`.
  - Resolution: Roadmap header description updated to match Architecture.

- [DONE] Roadmap version-mismatch handling conflicts with Architecture strictness
  - Evidence: `ROADMAP.md` now lists "Version mismatch handling (reject)".
  - Resolution: Roadmap item updated to reflect strict runtime contract.

Resolved:
- Roadmap checksum algorithm conflicts with Architecture
- Roadmap version-mismatch handling conflicts with Architecture strictness

Notes:
- Timestamp (UTC): 2025-12-29T17:55:29Z
- Limitations: Alignment review only; no new architectural content added.

### Review 2025-12-29T17:42:57Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Documents reviewed: `Architecture.md`, `ROADMAP.md`

Summary:
- Identified two mismatches between the protocol/compatibility expectations in Architecture and Roadmap.
- ROADMAP updated to align checksum algorithm and version-mismatch behavior with Architecture.

Findings:
- [NEW] Roadmap checksum algorithm conflicts with Architecture
  - Architecture expectation: PVS checksum uses SHA-256 (`Architecture.md`, PVS header defines algorithm id 1 = SHA-256).
  - Roadmap item: Phase 3 response headers list `X-Pavis-Checksum` as xxhash.
  - Incompatibility: Relay checksum headers should reflect the PVS SHA-256 payload checksum, not xxhash.
  - Roadmap adjustment: Update checksum header description to SHA-256 and add `X-Pavis-Checksum-Alg` to mirror the protocol header.

- [NEW] Roadmap version-mismatch handling conflicts with Architecture strictness
  - Architecture expectation: Runtime rejects any version mismatch as a hard error.
  - Roadmap item: Phase 2 lists "Version mismatch handling (reject vs warn)".
  - Incompatibility: Warning on mismatch violates the strict runtime contract.
  - Roadmap adjustment: Update item to "Version mismatch handling (reject)".

Resolved:
- None.

Notes:
- Timestamp (UTC): 2025-12-29T17:42:57Z
- Limitations: Alignment review focused on explicit protocol and runtime contract statements.

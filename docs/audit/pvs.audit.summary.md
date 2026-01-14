Audit Phase: PVS Audit
Target Crate: crates/pavis-pvs
Generation Timestamp: 2026-01-14T12:05:00Z
AI Model: Gemini

# 1. Executive Verdict

**Verdict:** Sound

The `pavis-pvs` crate implements a secure, robust, and opaque binary protocol for configuration distribution. It strictly enforces the boundary between artifact layout and business logic, delegating semantic interpretation to `pavis-core` while handling integrity (SHA-256) and layout validation (`rkyv`) itself. The code is defensive, with excellent error diagnostics and appropriate limits (e.g., 100MB payload cap) to prevent Denial of Service. While `mmap` usage introduces theoretical safety risks from external file modification, this is a standard and acceptable trade-off for zero-copy performance in this context.

# 2. Top System Risks

1.  **Memory Mapping Safety (Phase 4):**
    The use of `mmap` for reading configuration files (`verify_file`, `load`) is inherently unsafe if the underlying file is truncated or modified by another process during access. This can technically lead to Undefined Behavior (SIGBUS), though it is the standard mechanism for high-performance loaders.
2.  **Hardcoded Payload Limit (Phase 2):**
    The 100MB `MAX_PAYLOAD_SIZE` constant is hardcoded. While a sensible default for DoS protection, legitimate configurations exceeding this size will be rejected with no runtime override capability.
3.  **Double-Scan Overhead (Phase 5):**
    Validation requires two full passes over the payload: one for SHA-256 checksum verification and a second for `rkyv` structural validation. This linear `O(2N)` cost is acceptable for reliability but represents a fixed performance floor.

# 3. Readiness Assessment

| Criteria | Status | Notes |
| :--- | :--- | :--- |
| **Format Invariants Enforced?** | **Yes** | Magic bytes, Version, Algorithm, Header Size, Checksum, and Payload Size are all strictly checked. |
| **Diagnosable Errors?** | **Yes** | `PvsError` provides granular, context-rich error messages (e.g., hex-formatted checksum diffs, byte counts). |
| **Safe Malformed Input?** | **Yes** | `rkyv::check_archived_root` ensures structural safety. Size limits prevent OOM. |
| **Versioning Strategy?** | **Yes** | Explicit version field (`u32`) enforced to match `PAVIS_VERSION` (0). |

# 4. Recommended Next Steps

1.  **Configurable Limits:** Consider making `MAX_PAYLOAD_SIZE` configurable (e.g., via compile-time env var or builder API) to support exceptional use cases without code changes.
2.  **Zero-Copy Verification API:** The `verify(&[u8])` function currently clones the data to return an owned `VerifiedPvs`. Adding a `verify_ref` variant that returns a borrow would allow zero-copy verification of in-memory buffers.
3.  **Fuzz Testing:** While `rkyv` is robust, adding a fuzz target that generates random headers and payloads would further certify the resilience of the validation logic.

# 5. Detailed Analysis

## Phase 0: Inventory & Artifact Surface
-   **Responsibility:** Purely handles the `.pvs` binary envelope (Header + Rkyv Payload).
-   **API:** Clean separation. `header.rs` defines the wire format. `verify.rs` and `write.rs` handle I/O and checks.
-   **Dependencies:** `sha2` (integrity), `memmap2` (perf), `rkyv` (layout), `thiserror` (errors). Minimal and focused.

## Phase 1: Boundary & Responsibility Audit
-   **Semantic Logic:** **PASSED.** The crate treats the payload as an opaque byte slice, only casting it to `RuntimeConfig` for structural validation (`check_archived_root`). It never inspects fields of the config.
-   **I/O:** **PASSED.** Restricted to reading/writing artifact files.

## Phase 2: Format Invariants & Correctness
-   **Magic Bytes:** `PAVS` (Checked).
-   **Version:** 0 (Checked).
-   **Checksum:** SHA-256 (Checked).
-   **Size:** Header (64B) + Payload (<100MB).
-   **Enforcement:** All checks occur in `verify_bytes` before any data is exposed to the consumer.
    ```rust
    // crates/pavis-pvs/src/verify.rs
    if bytes.len() > HEADER_SIZE + MAX_PAYLOAD_SIZE {
        return Err(PvsError::PayloadTooLarge { ... });
    }
    ```

## Phase 3: Error Model & Diagnostics
-   **Quality:** Excellent. Hex formatting for checksums aids debugging.
    ```rust
    // crates/pavis-pvs/src/verify.rs
    return Err(PvsError::ChecksumMismatch {
        expected: to_hex(&header.checksum),
        found: to_hex(&computed_checksum),
    });
    ```
-   **Panic Policy:** Safe. `unwrap` in `parse_header` is protected by length checks.

## Phase 4: Safety & Malformed Input Resistance
-   **Unsafe:** `unsafe { Mmap::map(&file) }`. This is encapsulated in `read_from_path` and `verify_file`.
-   **DoS:** Protected by `MAX_PAYLOAD_SIZE`.
-   **Memory Safety:** `rkyv` ensures that even if the checksum passes (collision), the structural layout of the payload is valid before access.

## Phase 5: Compatibility & Performance
-   **Evolution:** `_reserved: [u8; 20]` in header allows for future extensions (e.g., compression flags) without breaking header layout.
-   **Performance:**
    -   `write`: Allocates 2x payload size (Serializer + Output Buffer).
    -   `read`: Zero-copy via `mmap`.
    -   `verify`: Linear scan.
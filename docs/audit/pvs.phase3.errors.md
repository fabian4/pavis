Audit phase name: Phase 3: Error Model & Diagnostics
Target crate: crates/pavis-pvs
Generation timestamp: 2026-01-07T10:39:00Z
AI model identifier: unknown

# Phase 3: Error Model & Diagnostics

## 1. Error Inventory

| Error Type | Location | Purpose |
| :--- | :--- | :--- |
| `PvsError` | `src/error.rs` | Comprehensive enum covering I/O, format identification, versioning, checksums, and archive corruption. |
| `PvsResult<T>` | `src/error.rs` | Standard Result alias for the crate. |

## 2. Context Quality

| Variant | Quality | Context Provided |
| :--- | :--- | :--- |
| `TooSmall` | **Excellent** | Includes `min` required bytes and `actual` bytes found. |
| `VersionMismatch` | **Excellent** | Includes `file` version and `expected` version. |
| `UnsupportedAlgorithm` | **Excellent** | Includes the algorithm ID found in the file. |
| `CorruptArchive` | **Good** | Includes the underlying error message from the `rkyv` validation engine. |
| `InvalidMagic` | **Poor** | Does not specify what bytes were found instead of the expected magic sequence. |
| `ChecksumMismatch` | **Poor** | Does not provide the expected vs. computed checksum values, which makes external debugging harder. |

## 3. Panic Policy

### Validation Path Panics
The following `unwrap()` calls are present in the core validation path (`src/read.rs`):

| File | Symbol | Snippet | Assessment |
| :--- | :--- | :--- | :--- |
| `src/read.rs` | `parse_header` | `let magic = buf[0..4].try_into().unwrap();` | **Logically safe but against policy**. The buffer size is checked by callers, but `unwrap()` on slice conversions is forbidden in strict validation paths. |
| `src/read.rs` | `parse_header` | `buf[4..8].try_into().unwrap()` (and others) | **Logically safe but against policy**. Multiple instances for `version`, `algorithm`, `checksum`, and `_reserved`. |

### Other Panic Sources
- `src/error.rs`: `unreachable!` inside `From<Infallible>`. This is safe as `Infallible` can never be instantiated.
- **Test Code**: Extensive use of `unwrap()`, `expect()`, and `expect_err()` is found throughout `mod tests` in all files, which is considered acceptable for test assertions.

## 4. Diagnostics Surface
- **Stable Display**: `thiserror` provides consistent and readable error messages.
- **No Sensitivity Leak**: Errors contain metadata (versions, lengths, checksums) but no configuration content or system secrets.
- **Consistency**: The error taxonomy is well-defined and maps 1:1 to the format invariants.

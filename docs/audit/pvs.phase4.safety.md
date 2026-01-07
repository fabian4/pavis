Audit phase name: Phase 4: Safety & Malformed Input Resistance
Target crate: crates/pavis-pvs
Generation timestamp: 2026-01-07T10:41:00Z
AI model identifier: unknown

# Phase 4: Safety & Malformed Input Resistance

## 1. Unsafe Code Review

| Location | Symbol | Justification | Assessment |
| :--- | :--- | :--- | :--- |
| `src/verify.rs` | `read_from_path`, `verify_file`, `load` | Uses `unsafe { Mmap::map(&file) }` to memory-map the PVS file for zero-copy access. | **Justified but risky**. Standard for high-performance I/O, but external file truncation while the map is active can lead to a process crash (SIGBUS). |

## 2. Panic & Crash Risks

- **Fixed Indexing**: The `parse_header` function in `src/read.rs` uses fixed slice indexing (e.g., `buf[0..4]`). This is verified safe because all call sites enforce the `HEADER_SIZE` (64 bytes) boundary before invocation.
- **Arithmetic Overflow**: No manual length arithmetic or pointer offsets were found. The crate relies on Rust's slice boundaries and `rkyv`'s internal validation.
- **Checksum Verification**: The checksum is computed using the `sha2` crate over the entire payload. This is a robust operation that does not involve unsafe memory access.

## 3. Memory Safety

- **Out-of-Bounds Reads**: Prevented by explicit length checks in `verify_bytes` and the structural validation provided by `rkyv::check_archived_root`.
- **Aliasing**: The crate uses immutable borrows for verification. `VerifiedPvs` maintains ownership of the underlying bytes (either `Vec<u8>` or `Mmap`), ensuring lifetimes are correctly managed.
- **Archive Safety**: The crate correctly uses `rkyv::check_archived_root`, which is the hardened entry point for rkyv, protecting against malicious offsets or cyclic references in the binary data.

## 4. DoS Vectors

- **Large Payload Hashing**: The `verify_bytes` function computes a SHA-256 hash of the entire payload. Since there is no enforced upper bound on payload size, a maliciously large artifact (multiple gigabytes) will cause high CPU usage and long blocking times during the verification phase.
- **Oversized Allocation**: If `verify` is called with a large byte slice, `verify_owned` will call `bytes.to_vec()`, causing a large allocation. This could lead to OOM (Out Of Memory) if the input size is not capped by the transport layer.

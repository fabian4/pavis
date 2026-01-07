Audit phase name: Phase 5: Compatibility & Performance Signals
Target crate: crates/pavis-pvs
Generation timestamp: 2026-01-07T10:43:00Z
AI model identifier: unknown

# Phase 5: Compatibility & Performance Signals

## 1. Versioning Strategy

- **Explicit Version Field**: `PvsHeader` includes a `version` (u32) field.
- **Strict Equality**: `verify_bytes` enforces an exact match against `PAVIS_VERSION`. This implies a "fail-fast" strategy for version mismatches.
- **Reserved Space**: 20 bytes are reserved in the header for future extensions without breaking the fixed 64-byte layout.

## 2. Schema Evolution Risks

- **Header Rigidness**: Any changes to the order or size of existing fields in `PvsHeader` (magic, version, algorithm, checksum) will break all existing `.pvs` files.
- **Implicit Payload Schema**: The PVS container does not store a schema ID for the inner configuration. If `RuntimeConfig` changes in `pavis-core` in a way that breaks `rkyv` compatibility, the version in the PVS header MUST be incremented, as there is no other way to detect schema drift.

## 3. Performance Signals

- **Memory Mapping**: The use of `memmap2` for reading PVS files is an excellent signal for performance, allowing the OS to handle paging and avoiding large userspace allocations for big configurations.
- **Zero-Copy Intent**: The integration with `rkyv` supports the system goal of zero-copy configuration access.
- **Clone Hotspot**: `VerifiedPvs` implements `Clone` by converting `Mmap` data into an owned `Vec<u8>`. If a downstream consumer (like a relay) clones a verified artifact multiple times, it will inadvertently trigger large memory allocations and copies, negating the benefits of memory mapping.
- **Checksum Bottleneck**: SHA-256 computation is performed synchronously on the main thread during `verify`. For very large artifacts, this will block the executor.

## 4. Bench Plan

| Bench Name | Target | Input Shape | Metric Intent |
| :--- | :--- | :--- | :--- |
| `bench_verify_small` | `verify()` | 1 KB artifact | Baseline overhead for header parsing and hashing. |
| `bench_verify_large` | `verify()` | 10 MB artifact | Impact of hashing and `rkyv` validation on latency. |
| `bench_mmap_vs_owned` | `read_from_path` vs `verify` | 5 MB file | Compare performance of memory mapping vs. reading into an owned vector. |
| `bench_encode` | `encode()` | Medium Config | Measure cost of `rkyv` serialization and checksumming. |
| `bench_verified_pvs_clone` | `VerifiedPvs::clone` | 1 MB Mapped | Measure the hidden cost of the Mmap-to-Owned conversion in the clone implementation. |

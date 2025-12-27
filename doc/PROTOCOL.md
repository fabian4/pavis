# Pavis Protocol (PVS) & Schema Evolution

This document describes the binary protocol used by Pavis for configuration distribution and the strategy for handling schema migrations and backwards compatibility.

## 1. File Format Structure

The `.pvs` file is a zero-copy binary blob optimized for fast loading via `mmap`.

| Offset | Size | Type | Value | Description |
|--------|------|------|-------|-------------|
| `0x00` | 4 | `[u8; 4]` | `PAVS` | Magic bytes – identifies file type |
| `0x04` | 4 | `u32` | `V` | Version – schema version for compatibility |
| `0x08` | ... | `bytes` | ... | Payload – the `ArchivedProxyConfig` root |

## 2. Versioning Strategy

Pavis uses a simple monotonically increasing integer for the protocol version (`PAVIS_VERSION` in `pavis-core`).

### Breaking Changes
- Any change to the `ProxyConfig` struct or its children that changes the binary layout (e.g., adding/removing fields, changing enum variants) requires incrementing the `PAVIS_VERSION`.
- Since `rkyv` is highly sensitive to struct layout, almost any structural change is considered breaking.

### Non-Breaking Changes
- Adding documentation or internal helper methods to the Rust structs that do not affect the `Archive` implementation.

## 3. Migration Strategy

As Pavis is designed for high-performance sidecar environments, we prioritize speed and simplicity over complex in-place migrations.

### Bridge-Side Migration (pavis-xds)
The `pavis-xds` component is responsible for translating user intent (YAML or xDS) into the current protocol version.
- When the protocol version is updated, `pavis-xds` must be redeployed first.
- `pavis-xds` should ideally support reading older YAML formats (via Serde aliases or manual mapping) but will always output the *current* `.pvs` version.

### Proxy-Side Handling (pavis)
The `pavis` proxy performs a strict version check during startup and config reload.

1. **Magic Byte Validation**: If magic bytes are not `PAVS`, the config is rejected.
2. **Version Check**:
   - **Exact Match**: If the file version matches the proxy's internal version, the config is loaded.
   - **Mismatch**: If the version differs, the proxy logs a critical error.
     - In a **Pod restart**, the proxy will fail to start if the local disk config is incompatible.
     - In a **Hot reload**, the proxy will reject the new config and continue serving with the old one.

## 4. Backwards Compatibility

We do not currently support "N-1" version compatibility in a single binary. 

### Recommended Upgrade Path
1. **Upgrade Control Plane**: Redeploy `pavis-xds`. It will start generating the new version of `.pvs` files.
2. **Rolling Update of Proxies**: Kubernetes will perform a rolling update of the application pods. New pods will pull the new config version from `pavis-xds`.
3. **Graceful Failure**: If a new proxy tries to load an old `.pvs` from disk (e.g., after a node failure), it will detect the version mismatch.

### Future Improvements
- **Schema Reflection**: We may investigate `rkyv` reflection or a more flexible schema (like FlatBuffers) if N-1 compatibility becomes a hard requirement.
- **Conversion Tooling**: `pavis-cli` will include a `convert` command to migrate `.pvs` files between versions for offline debugging.

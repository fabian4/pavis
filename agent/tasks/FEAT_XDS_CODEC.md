# Task: Implement `pavis-codec-xds`

## 1. Requirements

**Purpose**: Develop a pure transformation codec that converts Envoy xDS resource snapshots into the canonical `pavis-core::RuntimeConfig`.

**Integration**: This crate must integrate with `pavis-relay` via the pipeline orchestration (see `FEAT_RELAY_PIPELINE.md`).

**Inputs**:
- `pavis_ingest_api::Artifact`: Containing a serialized Protobuf payload (`XdsSnapshot`).
- `XdsSnapshot`: A collection of LDS, RDS, CDS, and EDS resources.

**Outputs**:
- `pavis_core::RuntimeConfig`: The internal domain model for the data plane.
- `pavis_core::ValidatedRuntimeConfig`: The outcome of successful semantic validation.

**Constraints**:
- **Purity**: No I/O, no network, no filesystem access.
- **Strictness**: Rejects inline certificates (must be file-based).
- **Structure**: Maps Envoy Listeners to `pavis-core::Listener` (supports LDS).
- **Stateless**: Does not maintain state between `compile` calls.

---

## 2. Guidelines

- **Standards**: Adhere to `ARCHITECTURE.md` (Split Data Plane).
- **Architecture**:
  - Implement the `Codec` trait from `pavis-codec-api`.
  - Use `pavis-core` as the source of truth for all types and validation logic.
  - Mimic the structure of `pavis-codec-serde`.
- **Dependencies**:
  - Use `prost` and `prost-types` for Protobuf handling.
  - Use `prost-build` in a `build.rs` to generate Rust structs from Envoy v3 Protos.
- **Safety**: Use `rkyv` compatibility through `pavis-core` traits; do not implement custom serialization.

---

## 3. Design Document

### Architecture Design
The codec follows the "Intermediate Type" pattern. 
1. **Decode**: Unmarshal Protobuf bytes into generated Envoy v3 structs.
2. **Normalize**: Map scattered xDS resources into a coherent internal representation.
3. **Map**: Transform Envoy structs to `pavis-core` structs.
4. **Validate**: Invoke `pavis_core::validate_runtime` to ensure semantic correctness.

### Data Models
The primary mapping target is `pavis_core::RuntimeConfig`:
- **LDS/HCM** -> `Vec<Listener>` + `TelemetryConfig`.
- **RDS** -> `Vec<VirtualHost>`.
- **CDS + EDS** -> `Vec<Upstream>` (Joined by cluster name).

### Error Handling
Use `pavis_codec_api::CodecError`:
- **Check**: Protobuf decoding failures or missing mandatory root resources.
- **Compile**: Mapping failures (e.g., unsupported LB policy, inline certs).
- **Core**: Semantic validation failures propagated from `pavis-core`.

---

## 4. Acceptance Criteria

- **Functionality**: Successfully maps LDS, RDS, CDS, and EDS resources into a single `RuntimeConfig`.
- **Completeness**:
  - HCM linkage correctly finds Route Configurations by name.
  - EDS endpoints are correctly associated with Clusters.
  - Telemetry (Access Logs) is extracted from the HCM filter.
- **Performance**: Compilation of a 10MB xDS snapshot must be efficient (zero-copy where feasible, minimal cloning).
- **Security**: Rejects malformed Protobuf or `Any` types that do not match the expected schema.
- **Compatibility**: Rejects protocol versions other than Envoy v3.

---

## 5. E2E Tests (Integration Level)

*Note: As this is a pure library, E2E refers to full-flow transformation tests within the crate.*

- **Full-Flow**: Given a binary `DiscoveryResponse` payload, verify the generated `RuntimeConfig` matches the expected JSON/YAML representation.
- **Integration**: Verify that `pavctl gen --format xds` (after modification to support the new codec) produces identical results to manual mapping.
- **Edge Case**: Snapshot with missing EDS resources for a CDS cluster (should result in empty upstream endpoints).
- **Limit Test**: Snapshot with 1,000+ routes and 10,000+ endpoints.

---

## 6. Test Cases

| Category | Case | Expected Result |
| :--- | :--- | :--- |
| **Functional** | Multi-weighted cluster split | Accurate weight distribution in `WeightedDestination`. |
| **Functional** | Access log to Stdout | `TelemetryConfig.access_log` set to `Stdout`. |
| **Boundary** | Empty Route Table | Valid `RuntimeConfig` with 0 routes. |
| **Negative** | Inline Certificate Bytes | `CodecError::Compile` (Unsupported). |
| **Negative** | Mismatched RDS Name | `CodecError::Compile` (RouteConfig not found). |
| **Regression** | Health Status Filtering | `UNHEALTHY` endpoints are excluded from the `Upstream` list. |
| **Regression** | Regex Path Match | `MatchType::Regex` correctly assigned with raw pattern. |
| **Regression** | Binary Mode E2E | Run with `TEST_MODE=binary make e2e-relay`. |

## 7. Progress Log

- **2026-01-04**:
  - Refactored `pavis-core` to replace `ServerConfig` with `Vec<Listener>` to support multiple listeners (LDS alignment).
  - Updated `pavis` runtime to bootstrap multiple listeners from configuration.
  - Updated `pavis-relay` and `pavctl` to support the new `RuntimeConfig` schema.
  - `pavis-e2e` helpers updated for new config structure.

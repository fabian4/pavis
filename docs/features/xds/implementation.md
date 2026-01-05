# Implementation Plan: Pavis xDS Readiness (Revised)

This document outlines the remaining work required to complete full compatibility with the `pavis-codec-xds` transformation layer.

## 1. Remaining Work: xDS Codec (`pavis-codec-xds`)

This section defines the concrete translation work required to map Envoy xDS snapshots into the current `pavis-core` schema.

### Architecture Design: The Intermediate Type Pattern
The codec follows a structured background transformation pipeline:
1. **Decode**: Unmarshal Protobuf bytes into generated Envoy v3 structs using `prost`.
2. **Normalize**: Map scattered xDS resources into a coherent internal representation.
3. **Map**: Transform Envoy structs to `pavis-core` structs (Listeners, Upstreams, VirtualHosts).
4. **Validate**: Performed by the codec pipeline in `Codec::materialize` via `pavis_core::validate_runtime`.

### A. LDS -> Listener Mapping
- **Input**: LDS `Listener` resources.
- **Output**: `RuntimeConfig.listeners: Vec<Listener>`.
- **Rules**:
  - 1:1 mapping from Envoy listener name to `Listener::name`.
  - Listen address must be extracted from a single socket address; multiple addresses are unsupported.
  - Reject listeners that contain:
    - Multiple filter chains.
    - SNI-based chain matching.
    - Any dynamic listener matching features.
  - Error type: `CodecError::UnsupportedFeature` with explicit reason.

### B. CDS + EDS -> Upstream Mapping
- **Input**: CDS `Cluster` and EDS `ClusterLoadAssignment` resources.
- **Output**: `RuntimeConfig.upstreams: Vec<Upstream>`.
- **Rules**:
  - Cluster name maps to `Upstream::name`.
  - `cluster_type` maps to `DiscoveryType`:
    - `STATIC` -> `Static`
    - `LOGICAL_DNS` -> `LogicalDns`
    - `STRICT_DNS` -> `StrictDns`
    - `EDS` -> `Static` (In Pavis, EDS endpoints are flattened into the static upstream definition during transformation).
  - Endpoints:
    - `STATIC`: endpoints must be IP literals, map to `EndpointAddr::Ip`.
    - `LOGICAL_DNS`/`STRICT_DNS`: endpoints must be hostnames, map to `EndpointAddr::Dns`.
    - `EDS`: endpoints are extracted from `ClusterLoadAssignment` by joining on cluster name.
  - Health Filtering: `UNHEALTHY` endpoints are excluded from the `Upstream` list.
  - `load_balancing_policy` maps to `LoadBalancer` using a strict supported subset (document supported values).
  - `http_protocol_options` map to `HttpVersion` with explicit defaults.

### C. RDS -> Routes Mapping
- **Input**: RDS `RouteConfiguration` resources.
- **Output**: `RuntimeConfig.routes: Vec<VirtualHost>`.
- **Rules**:
  - Each `virtual_host` maps to `VirtualHost::host` and `VirtualHost::paths`.
  - Supported matches:
    - Prefix match -> `PathMatch::Prefix`.
    - Exact match -> `PathMatch::Exact`.
    - Regex match -> `PathMatch::Regex` (ensure regex length limit aligns with core validation).
  - Route ordering preserved from xDS.
  - Reject unsupported match types with `CodecError::UnsupportedFeature`.
  - Header actions:
    - `request_headers_to_add` maps to `Headers.set_headers` or `Headers.append_headers` based on `append`.
    - `response_headers_to_add` maps to `Headers.set_headers` or `Headers.append_headers` based on `append`.
    - `headers_to_remove` maps to `Headers.remove_headers`.
  - Rewrite rules:
    - `prefix_rewrite` -> `RewritePath::Prefix`.
    - `host_rewrite_literal` -> `RewriteHost::Literal`.
  - Destinations:
    - Weighted clusters map to `Destination`.
    - Ensure referenced upstreams exist; otherwise reject with a clear error.
  - Timeouts and retries:
    - Map `timeout` and retry policy into `Route::timeout` and `RetryPolicy` with explicit defaults.

### D. Canonical Defaults & Validation Surface
- **Defaults**:
  - Timeouts and retry policy defaults must be deterministic.
  - Connection pool defaults must be set explicitly in the codec.
- **Validation**:
  - Ensure all required fields for core validation are populated.
  - Reject unknown/unsupported enum values early.
  - Provide clear error context: resource type, name, and field path.

---

## 2. Responsibility Matrix

| Component | Responsibility | Boundary |
| :--- | :--- | :--- |
| **Ingest** | Network I/O with xDS server. | Passes raw xDS Protobuf -> Codec. |
| **Codec** | Pure translation. | Maps Proto enums -> Core enums. 1:1 mapping. No DNS resolution. |
| **Core** | Domain definitions. | Defines *what* a DNS upstream is (the type), but NOT *how* it resolves or refreshes. |
| **Runtime** | Execution & IO. | Handles DNS refresh intervals, TTL, and address family selection. Holds mutable state. |

---

## 3. Implementation Phasing

### Phase 1: `pavis-codec-xds` Translation
*Goal: Produce a valid `RuntimeConfig` from xDS snapshots.*
1.  **LDS Mapping**: Implement listener extraction + reject unsupported filter chains.
2.  **CDS Mapping**: Implement cluster translation + discovery type mapping + endpoint addressing.
3.  **RDS Mapping**: Implement route, header, rewrite, timeout, retry, and weighted destination translation.
4.  **Defaulting**: Apply deterministic defaults for missing values.
5.  **Validation**: Ensure resulting `RuntimeConfig` passes core validation before emission.

---

## 4. Risks & Trade-offs

1.  **DNS Latency**: Resolution is async, but initial resolution might delay startup or first request. The runtime should start "healthy" but fail requests to DNS upstreams until the first resolution completes.
2.  **Rewrite Complexity**: `path_prefix_rewrite` depends heavily on accurate normalization of the request path. We must ensure the "matched prefix" is tracked accurately during routing.

---

## 5. Acceptance Criteria

- **Functionality**: Successfully maps LDS, RDS, CDS, and EDS resources into a single `RuntimeConfig`.
- **Completeness**:
  - HCM linkage correctly finds Route Configurations by name.
  - EDS endpoints are correctly associated with Clusters.
  - Telemetry (Access Logs) is extracted from the HCM filter.
- **Performance**: Compilation of a 10MB xDS snapshot must be efficient (zero-copy where feasible, minimal cloning).
- **Security**: Rejects malformed Protobuf or `Any` types that do not match the expected schema.
- **Compatibility**: Rejects protocol versions other than Envoy v3.

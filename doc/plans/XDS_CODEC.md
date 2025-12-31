# Draft Plan: `pavis-codec-xds`

This crate implements the **Transformation Layer** for the xDS pipeline. It is responsible for converting raw xDS resource snapshots (emitted by `pavis-ingest-xds`) into the canonical `pavis-core::RuntimeConfig` format.

> **Design Constraint:** This codec is **pure**. It must not perform I/O, network calls, or side effects. It accepts a complete state-of-the-world (Snapshot) and produces a valid configuration or an error.

---

## 1. Crate Architecture

**Location**: `crates/pavis-codec-xds`

### Dependencies
*   **`prost`**, **`prost-types`**: For handling Protocol Buffers and `Any` types.
*   **`pavis-core`**: Target domain types (`RuntimeConfig`, etc.).
*   **`pavis-codec-api`**: Codec traits (`Codec`, `CheckedArtifact`).
*   **`prost-build` (dev)**: To compile Envoy protobufs.

### Proto Strategy
To maintain the "No External Magic" rule and minimize bloat:
1.  **Vendor Minimal Protos**: We will vendor only the necessary Envoy v3 `.proto` files (LDS, RDS, CDS, EDS, Core, HCM) into `crates/pavis-codec-xds/proto/`.
2.  **Generate at Build Time**: Use `build.rs` with `prost-build` to generate Rust structs during compilation.
3.  **Input Schema**: The codec will expect an `Artifact` containing a serialized `XdsSnapshot` (defined below), likely encoded as a Protobuf message containing lists of resources.

---

## 2. Input Data Model (`XdsSnapshot`)

Since xDS resources (Listeners, Clusters, Routes) arrive independently over the network, the Ingest layer (`pavis-ingest-xds`) is responsible for aggregating them. The Codec expects a coherent **Snapshot**:

```protobuf
// Conceptual Proto definition for the Artifact payload
message XdsSnapshot {
  repeated envoy.config.listener.v3.Listener listeners = 1;
  repeated envoy.config.route.v3.RouteConfiguration route_configs = 2;
  repeated envoy.config.cluster.v3.Cluster clusters = 3;
  repeated envoy.config.endpoint.v3.ClusterLoadAssignment load_assignments = 4;
}
```

---

## 3. Mapping Specification (xDS → RuntimeConfig)

This section details the specific field mappings from Envoy xDS resources to Pavis `RuntimeConfig`.

### 3.1. LDS → `ServerConfig`

Pavis currently supports a single server listener. The codec will select the **first** valid listener or filter by a convention (e.g., name="pavis_listener").

| Envoy (`Listener`) | Pavis (`ServerConfig`) | Notes |
| :--- | :--- | :--- |
| `address.socket_address` | `listen_addr` | Maps IP and Port. |
| `filter_chains[].transport_socket` | `tls` | If present, enables TLS. |
| `transport_socket...common_tls_context` | `tls.cert_path`, `tls.key_path` | **Constraint**: Only `filename` datasources are supported. Inline bytes will cause a compilation error. |

#### 3.1.1 HttpConnectionManager (HCM)
The `HttpConnectionManager` filter is required to link the Listener to Routes and Telemetry.

| Envoy (`HttpConnectionManager`) | Pavis Usage | Notes |
| :--- | :--- | :--- |
| `rds.route_config_name` | Linkage | Used to select the correct `RouteConfiguration` from the snapshot. |
| `route_config` (Inline) | Routing | If present, used directly as the route table. |
| `access_log` | Telemetry | Maps to `TelemetryConfig.access_log` (see 3.4). |
| `tracing` | Telemetry | Maps to `TelemetryConfig.tracing` (see 3.4). |

### 3.2. RDS → `routes: Vec<VirtualHost>`

Envoy separates `Listener` -> `RouteConfiguration` -> `VirtualHost`. Pavis flattens this.

| Envoy (`VirtualHost` / `Route`) | Pavis (`VirtualHost` / `Route`) | Notes |
| :--- | :--- | :--- |
| `VirtualHost.domains` | `VirtualHost.host` | `*` matches are preserved. |
| `VirtualHost.routes` | `VirtualHost.paths` | Ordered list is preserved. |
| **Match** | | |
| `match.prefix` | `MatchType::Prefix` | |
| `match.path` | `MatchType::Exact` | |
| `match.safe_regex` | `MatchType::Regex` | |
| **Action** | | |
| `route.cluster` | `destinations` | Single destination with weight 1. |
| `route.weighted_clusters` | `destinations` | Maps `name` -> `upstream`, `weight` -> `weight`. |
| `route.timeout` | `timeout_ms` | Converted to milliseconds. |
| `route.retry_policy` | `RetryPolicy` | `num_retries`->`attempts`, `retry_on` maps strings directly. |
| `route.request_headers_to_add` | `request_headers.add` | `HeaderValueOption`. Note: `append` flag is ignored (Pavis overwrites/inserts). |
| `route.request_headers_to_remove` | `request_headers.remove` | |

### 3.3. CDS + EDS → `upstreams: Vec<Upstream>`

Pavis combines Cluster configuration (CDS) and Endpoint state (EDS) into a single `Upstream` object. The codec must join these resources by cluster name.

| Envoy (`Cluster`) | Pavis (`Upstream`) | Notes |
| :--- | :--- | :--- |
| `name` | `name` | Primary key for joining CDS/EDS. |
| `lb_policy` | `load_balancer` | `ROUND_ROBIN` -> `RoundRobin`, `RANDOM` -> `Random`. Others default to `RoundRobin`. |
| `connect_timeout` | `connection_pool.connection_timeout_secs` | |
| `transport_socket` (UpstreamTls) | `tls` | Enables upstream TLS. |
| `http2_protocol_options` | `http_version` | Presence implies `H2`. Absence implies `H1`. |
| `type` | N/A | **Constraint**: Only `STATIC` and `EDS` supported. `LOGICAL_DNS` is not yet supported. |

| Envoy (`ClusterLoadAssignment`) | Pavis (`Endpoint`) | Notes |
| :--- | :--- | :--- |
| `endpoints[].lb_endpoints` | `endpoints` | Flattened list of endpoints. |
| `lb_endpoints[].health_status` | Filter | **Constraint**: Only `HEALTHY` or `UNKNOWN` endpoints are mapped. `UNHEALTHY` / `DRAINING` are dropped. |
| `endpoint.address.socket_address` | `ip`, `port` | Must resolve to an IP. Hostnames here are rejected. |
| `load_balancing_weight` | `weight` | Defaults to 1 if missing. |

### 3.4. Telemetry Mapping (from HCM)

| Envoy (`HttpConnectionManager`) | Pavis (`TelemetryConfig`) | Notes |
| :--- | :--- | :--- |
| `access_log` | `access_log` | Presence implies `Stdout` or `File` (if path type). |
| `tracing` | `tracing` | If present, maps to `TracingConfig`. `provider` inferred from `typed_config`. |

---

## 4. Limitations & Constraints

1.  **No Inline Certificates**: `pavis-core` requires file paths for certificates. xDS resources providing inline certificate bytes will fail to compile.
2.  **Single Listener**: If the snapshot contains multiple Listeners, the codec will process only the first one (or specific named one) and warn about others.
3.  **No DNS Upstreams**: `LOGICAL_DNS` and `STRICT_DNS` clusters cannot be mapped to `pavis-core`'s IP-based `Endpoint` model yet.
4.  **Header Append**: Envoy's `append` flag for headers is currently treated as "insert" (overwrite) by Pavis runtime.
5.  **Unsupported Route Actions**: `DirectResponse` (static replies), `Redirect`, and `Host/Path Rewrite` actions are not supported by `pavis-core` and will result in ignored routes or compilation errors.

## 5. Mapping Example

This section demonstrates how a complex xDS configuration (YAML representation of Protobuf) translates into the `pavis-core` `RuntimeConfig`.

### 5.1 Input: XdsSnapshot (Conceptual)

```yaml
# 1. Listener (LDS)
listeners:
  - name: "pavis-listener"
    address: { socket_address: { address: "0.0.0.0", port_value: 8080 } }
    filter_chains:
      - transport_socket: # TLS Enabled
          name: "envoy.transport_sockets.tls"
          typed_config:
            "@type": "type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext"
            common_tls_context:
              tls_certificates:
                - certificate_chain: { filename: "/etc/pavis/cert.pem" }
                  private_key: { filename: "/etc/pavis/key.pem" }
        filters:
          - name: "envoy.filters.network.http_connection_manager"
            typed_config:
              "@type": "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager"
              stat_prefix: "ingress_http"
              access_log:
                - name: "envoy.access_loggers.stdout"
              rds:
                route_config_name: "local_route"

# 2. Routes (RDS)
route_configs:
  - name: "local_route"
    virtual_hosts:
      - name: "backend"
        domains: ["*"]
        routes:
          - match: { prefix: "/service" }
            route:
              cluster: "backend-service"
              timeout: "5s"
              retry_policy:
                retry_on: "5xx"
                num_retries: 3
          - match: { safe_regex: { regex: "^/api/v[0-9]+$" } }
            route:
              weighted_clusters:
                clusters:
                  - name: "backend-v1"
                    weight: 90
                  - name: "backend-v2"
                    weight: 10

# 3. Clusters (CDS)
clusters:
  - name: "backend-service"
    type: EDS
    connect_timeout: "2s"
    lb_policy: ROUND_ROBIN
  - name: "backend-v1"
    type: STATIC
    load_assignment: # Inline EDS for STATIC
      cluster_name: "backend-v1"
      endpoints:
        - lb_endpoints:
            - endpoint: { address: { socket_address: { address: "10.0.0.10", port_value: 8080 } } }
              load_balancing_weight: 1
  - name: "backend-v2"
    type: EDS
    http2_protocol_options: {} # H2

# 4. Endpoints (EDS)
load_assignments:
  - cluster_name: "backend-service"
    endpoints:
      - lb_endpoints:
          - endpoint: { address: { socket_address: { address: "10.2.0.5", port_value: 80 } } }
            health_status: HEALTHY
            load_balancing_weight: 5
          - endpoint: { address: { socket_address: { address: "10.2.0.6", port_value: 80 } } }
            health_status: UNHEALTHY # Will be filtered out
  - cluster_name: "backend-v2"
    endpoints:
      - lb_endpoints:
          - endpoint: { address: { socket_address: { address: "192.168.1.50", port_value: 9000 } } }
```

### 5.2 Output: RuntimeConfig (Rust Debug Representation)

```rust
RuntimeConfig {
    server: ServerConfig {
        listen_addr: "0.0.0.0:8080",
        worker_threads: None,
        tls: Some(TlsConfig {
            enabled: true,
            cert_path: Some("/etc/pavis/cert.pem".into()),
            key_path: Some("/etc/pavis/key.pem".into()),
        }),
    },
    telemetry: TelemetryConfig {
        access_log: AccessLogConfig::Stdout,
        // ... defaults
    },
    upstreams: vec![
        // Joined CDS + EDS for "backend-service"
        Upstream {
            name: "backend-service".into(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1, // Default
            connection_pool: ConnectionPoolConfig {
                connection_timeout_secs: 2,
                ..Default::default()
            },
            endpoints: vec![
                Endpoint { ip: "10.2.0.5", port: 80, weight: 5 },
                // 10.2.0.6 dropped (UNHEALTHY)
            ],
            ..Default::default()
        },
        // STATIC cluster "backend-v1"
        Upstream {
            name: "backend-v1".into(),
            endpoints: vec![
                Endpoint { ip: "10.0.0.10", port: 8080, weight: 1 },
            ],
            ..Default::default()
        },
        // EDS cluster "backend-v2" (H2)
        Upstream {
            name: "backend-v2".into(),
            http_version: HttpVersion::H2,
            endpoints: vec![
                Endpoint { ip: "192.168.1.50", port: 9000, weight: 1 },
            ],
            ..Default::default()
        }
    ],
    routes: vec![
        VirtualHost {
            host: "*".into(),
            paths: vec![
                // Prefix Route
                Route {
                    match_type: MatchType::Prefix,
                    path: "/service".into(),
                    timeout_ms: Some(5000),
                    retry_policy: Some(RetryPolicy {
                        attempts: 3,
                        retry_on: vec!["5xx".into()],
                        // ...
                    }),
                    destinations: vec![
                        WeightedDestination { upstream: "backend-service".into(), weight: 1 }
                    ],
                    ..Default::default()
                },
                // Regex Route (Weighted Split)
                Route {
                    match_type: MatchType::Regex,
                    path: "^/api/v[0-9]+$".into(),
                    destinations: vec![
                        WeightedDestination { upstream: "backend-v1".into(), weight: 90 },
                        WeightedDestination { upstream: "backend-v2".into(), weight: 10 },
                    ],
                    ..Default::default()
                }
            ]
        }
    ]
}
```

## 6. Work Plan

1.  **Scaffold**: Create `crates/pavis-codec-xds`.
2.  **Protos**: Vendor minimal Envoy v3 protos into `crates/pavis-codec-xds/proto/` and configure `build.rs` with `prost-build`.
3.  **Data Model**: Define `XdsSnapshot` struct (or Proto) to act as the deserialization target for the `Artifact`.
4.  **Implementation**:
    *   Implement `Codec::check` to validate the protobuf payload.
    *   Implement `Codec::compile` to perform the transformations mapped above, including HCM parsing and EDS joining.
    *   Implement `Codec::pack` (optional, for debugging).
5.  **Testing**: Create unit tests with raw `DiscoveryResponse` bytes to verify accurate `RuntimeConfig` generation.

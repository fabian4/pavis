# Pavis Schema & Runtime Refactoring Plan

This plan outlines the refactoring of the Pavis configuration system to prioritize execution efficiency, binary compactness, and semantic clarity.

### 1. Summary of Changes

*   **Closed-Set String Elimination:** All fields with a fixed set of options (e.g., Load Balancer, HTTP Version) are converted to `#[repr(u8)]` enums to eliminate string comparisons on the hot path.
*   **Concise Renaming:** Keys are shortened (e.g., `load_balancer` → `lb`, `connection_pool` → `pool`) to reduce the overhead of the serialized schema while maintaining YAML aliases for human readability.
*   **Option Compaction:** Fields that must be materialized by the Codec (like timeouts or retry counts) are changed from `Option<T>` to explicit types. The Runtime no longer applies "hidden" defaults.
*   **Bit-Width Optimization:** Numeric types are right-sized (e.g., `u16` for weights, `u32` for millisecond timeouts) to maximize cache-line efficiency in the memory-mapped `.pvs` file.
*   **Explicit State Enums:** Introduced `WorkerCount` and `AccessLog` enums to cleanly separate "Disabled", "Auto", and "Explicit" states without relying on magic values like `-1` or `null`.

---

### 2. Enum Table (Old String → New Enum)

| Context | Old YAML String | New Enum Variant (repr u8) | RuntimeConfig Type |
| :--- | :--- | :--- | :--- |
| **Discovery** | "static", "strict-dns" | `Discovery::Static = 0`, `Discovery::StrictDns = 1` | `Discovery` |
| **Load Balancer** | "round-robin", "random" | `Lb::RoundRobin = 0`, `Lb::Random = 1` | `Lb` |
| **HTTP Version** | "h1", "h2", "auto" | `Http::H1 = 0`, `Http::H2 = 1`, `Http::Auto = 2` | `Http` |
| **Match Type** | "prefix", "exact", "regex" | `Match::Prefix = 0`, `Match::Exact = 1`, `Match::Regex = 2` | `Match` |
| **Access Log** | "stdout", "disabled" | `AccessLog::Stdout = 0`, `AccessLog::Disabled = 1` | `AccessLog` |
| **Header Action** | "set", "append", "remove" | `HeaderOp::Set = 0`, `HeaderOp::Append = 1`, `HeaderOp::Remove = 2` | `HeaderOp` |

---

### 3. Revised RuntimeConfig (Rust-like Definition)

```rust
// Core compact types for zero-copy execution
#[repr(u8)]
pub enum Discovery { Static = 0, StrictDns = 1, LogicalDns = 2 }

#[repr(u8)]
pub enum Lb { RoundRobin = 0, Random = 1, LeastRequest = 2 }

#[repr(u8)]
pub enum Http { H1 = 0, H2 = 1, Auto = 2 }

pub struct Pool {
    pub idle_s: u32,
    pub connect_s: u32,
    pub max_conns: u32,
}

pub struct Upstream {
    pub name: String,
    pub discovery: Discovery,
    pub lb: Lb,
    pub http: Http,
    pub pool: Pool,
    pub tls: Option<UpstreamTls>,
    pub endpoints: Vec<Endpoint>,
}

pub struct Endpoint {
    pub addr: EndpointAddr,
    pub w: u16, // weights fit in u16
}

pub enum EndpointAddr {
    Ip(SocketAddr),
    Dns(String, u16),
}

#[repr(u8)]
pub enum Match { Prefix = 0, Exact = 1, Regex = 2 }

pub struct Route {
    pub matcher: Match,
    pub path: String,
    pub timeout_ms: u32, // Explicit value, 0 = no timeout
    pub retry: Option<RetryPolicy>,
    pub headers: Option<HeaderOps>,
    pub rewrite: Option<Rewrite>,
    pub to: Vec<WeightedDest>,
}

pub struct WeightedDest {
    pub upstream: String,
    pub w: u16,
}
```

---

### 4. Revised YAML Example

This YAML is consumed by the **Codec**. The Codec uses `serde` aliases to support these concise names while allowing the string literals to map to the internal `repr(u8)` variants.

```yaml
listeners:
  - name: "prod-ingress"
    addr: "0.0.0.0:443"
    workers: auto # Codec maps to internal WorkerCount::Auto
    tls:
      cert: "/etc/pavis/tls.crt"
      key: "/etc/pavis/tls.key"

upstreams:
  - name: "api-v1"
    discovery: static
    lb: round-robin
    http: h1
    pool:
      idle_s: 60
      connect_s: 5
    endpoints:
      - addr: "10.0.1.5:8080"
        w: 100

routes:
  - host: "api.example.com"
    paths:
      - match: exact
        path: "/login"
        timeout_ms: 2500
        to:
          - upstream: "api-v1"
            w: 1
      - match: prefix
        path: "/static"
        to:
          - upstream: "api-v1"
            w: 1
```

---

### 5. Migration and Benefits

*   **PVS Schema Bump:** This is a breaking change for the `.pvs` binary format. The version field in the `PvsHeader` MUST be incremented.
*   **Codec Backward Compatibility:** The Codec can maintain support for "legacy" YAML keys (`load_balancer`, `http_version`) using `#[serde(alias = "...")]` attributes, ensuring zero-friction for existing user configurations.
*   **Hot Path Performance:** The Runtime now performs simple `u8` integer comparisons for routing and load balancing decisions, removing all string parsing and hashing from the request path.
*   **Memory Footprint:** The resulting configuration is significantly smaller and more cache-friendly, allowing larger route tables to fit within the same L3 cache footprint.

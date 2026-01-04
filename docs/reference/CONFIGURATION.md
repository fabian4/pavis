# Runtime Configuration Reference

> **Status:** Reference
> **Role:** Canonical definition of the fully materialized runtime configuration and its YAML form.

This document describes the `pavis_core::RuntimeConfig` structure consumed by the Pavis runtime and the YAML emitted/accepted by the serde codec.

## RuntimeConfig (Rust)
```rust
pub struct RuntimeConfig {
    pub listeners: Vec<Listener>,
    pub telemetry: Telemetry,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

pub struct Duration(pub NonZeroU32);

pub enum Timeout {
    Disabled,
    Enabled(Duration),
}

pub enum ConnectTimeout {
    Disabled,
    Enabled(Duration),
}

pub enum IdleTimeout {
    Disabled,
    Enabled(Duration),
}

pub enum TryTimeout {
    Inherit,
    Disabled,
    Enabled(Duration),
}

pub struct Hostname(pub String);
pub struct Host(pub String);
pub struct Path(pub String);
pub struct ServiceName(pub String);
pub struct HeaderName(pub String);
pub struct HeaderValue(pub String);
pub struct UpstreamName(pub String);
pub struct UpstreamId(pub NonZeroU16);
pub struct ListenerName(pub String);
pub struct Port(pub NonZeroU16);
pub struct Weight(pub NonZeroU16);
pub struct SampleRate(pub u32);

pub struct Listener {
    pub name: ListenerName,
    pub address: SocketAddr,
    pub workers: WorkerCount,
    pub tls: TlsConfig,
}

pub enum WorkerCount {
    Auto,
    Count(NonZeroU16),
}

pub enum TlsConfig {
    Disabled,
    Enabled { cert_path: Path, key_path: Path },
}

pub struct Telemetry {
    pub level: LogLevel,
    pub pingora: LogLevel,
    pub service_name: ServiceName,
    pub metrics: Metrics,
    pub access_log: AccessLogPolicy,
    pub tracing: TracingPolicy,
}

pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

pub enum AccessLogPolicy {
    Disabled,
    Stdout,
    File(Path),
}

pub enum TracingPolicy {
    Disabled,
    Enabled {
        provider: TracingProvider,
        sampling: SampleRate,
    },
}

pub enum TracingProvider {
    Otlp,
    Jaeger,
    Zipkin,
}

pub enum Metrics {
    Disabled,
    Enabled { addr: SocketAddr },
}

pub struct Upstream {
    pub id: UpstreamId,
    pub name: UpstreamName,
    pub discovery: Discovery,
    pub balancer: LoadBalancer,
    pub protocol: HttpVersion,
    pub pool: Pool,
    pub tls: TlsPolicy,
    pub endpoints: Vec<Endpoint>,
}

pub enum Discovery {
    Static,
    StrictDns,
    LogicalDns,
}

pub enum LoadBalancer {
    RoundRobin,
    Random,
    LeastRequest,
}

pub enum HttpVersion {
    H1,
    H2,
    H2H1,
}

pub struct Pool {
    pub idle: IdleTimeout,
    pub connect: ConnectTimeout,
    pub max: ConnectionLimit,
}

pub enum ConnectionLimit {
    Unlimited,
    Limited(NonZeroU32),
}

pub enum TlsPolicy {
    Disabled,
    Enabled { verify_mode: TlsVerify, sni: SniName },
}

pub enum TlsVerify {
    Disabled,
    Cert,
    CertAndHost,
}

pub enum SniName {
    Auto,
    Value(Hostname),
}

pub struct Endpoint {
    pub address: EndpointAddr,
    pub weight: Weight,
}

pub enum EndpointAddr {
    Ip { address: IpAddr, port: Port },
    Dns { host: Hostname, port: Port },
}

pub struct VirtualHost {
    pub host: Host,
    pub paths: Vec<Route>,
}

pub struct Route {
    pub matcher: PathMatch,
    pub timeout: Timeout,
    pub retry: RetryPolicy,
    pub request_headers: HeadersPolicy,
    pub response_headers: HeadersPolicy,
    pub rewrite: Rewrite,
    pub destinations: Vec<Destination>,
}

pub enum PathMatch {
    Prefix { path: Path },
    Exact { path: Path },
    Regex { path: Path },
}

pub enum RetryPolicy {
    Disabled,
    Enabled {
        attempts: NonZeroU16,
        per_try: TryTimeout,
        on: RetryFlags,
    },
}

pub struct RetryFlags(pub u8);
pub const RETRY_FIVE_XX: u8 = 0b0000_0001;
pub const RETRY_CONNECT_FAILURE: u8 = 0b0000_0010;
pub const RETRY_RESET: u8 = 0b0000_0100;
pub const RETRY_REFUSED: u8 = 0b0000_1000;
pub const RETRY_RESERVED: u8 = 0b1111_0000;

pub enum HeadersPolicy {
    Disabled,
    Enabled { rules: Headers },
}

pub struct Headers {
    pub set_headers: Vec<(HeaderName, HeaderValue)>,
    pub append_headers: Vec<(HeaderName, HeaderValue)>,
    pub add_headers: Vec<(HeaderName, HeaderValue)>,
    pub remove_headers: Vec<HeaderName>,
}

pub struct Rewrite {
    pub path: RewritePath,
    pub host: RewriteHost,
}

pub enum RewritePath {
    Disabled,
    Prefix { from: Path, to: Path },
}

pub enum RewriteHost {
    Disabled,
    Literal { host: Hostname },
}

pub struct Destination {
    pub upstream: UpstreamName,
    pub weight: Weight,
}
```

**Normative Semantics**
- `HeadersPolicy::Disabled` means no header mutations are applied.
- Regex compilation happens at runtime load/swap and is not stored in the schema.

## YAML Reference (serde codec)

```yaml
listeners:
  - name: "default"
    address: "0.0.0.0:8080"
    workers: null
    tls: null

telemetry:
  level: null
  pingora: null
  service_name: null
  metrics: null
  access_log: "stdout"   # "stdout", "false", or file path
  tracing:
    provider: "otlp"      # otlp, jaeger, zipkin
    sampling: 1000

upstreams:
  - id: 1                # optional
    name: "backend"
    discovery: "static"        # static, strict-dns, logical-dns
    balancer: "round-robin"    # round-robin, random, least-request
    protocol: "h1"            # h1, h2, h2h1
    pool:
      idle: "60s"
      connect: "5s"
      max: null                # null = unlimited
    tls:
      enabled: true
      verify_hostname: true
      verify_cert: true
      sni: "backend.local"
    circuit_breaker: null
    health_check: null
    endpoints:
      - address: "127.0.0.1"
        port: 8081
        weight: 1

routes:
  - host: "example.com"
    paths:
      - matcher:
          prefix:
            path: "/"
        timeout: null          # duration string or null
        retry:
          attempts: 2
          per_try_timeout: "250ms"
          retry_on: ["5xx", "connect_failure"]
        request_headers:
          set_headers:
            - ["x-added", "1"]
          append_headers: []
          add_headers: []
          remove_headers: ["x-remove"]
        response_headers: null
        rewrite:
          path_prefix_rewrite: "/v2"
          host_rewrite_literal: "backend.local"
        destinations:
          - upstream: "backend"
            weight: 1
```

Notes:
- YAML durations (`idle`, `connect`, `timeout`, `per_try`) accept human-friendly strings and are materialized into milliseconds in `RuntimeConfig`.
- Endpoint weights and destination weights are `NonZeroU16` in the runtime config.

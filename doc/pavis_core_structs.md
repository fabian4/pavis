# Pavis Core Structs

Quick reference for serialized types defined in `crates/pavis-core/src/lib.rs`. Authoritative definitions remain in code.

## `PavisHeader`
```
PavisHeader
├─ magic: [u8; 4]          // magic bytes "PAVS" to identify file type
├─ version: u32            // protocol version expected by binaries
├─ algorithm: u32          // checksum algorithm id (1 = SHA-256)
├─ checksum: [u8; 32]      // checksum over payload (header excluded)
└─ _reserved: [u8; 20]     // padding/reserved for future fields
```

## `RuntimeConfig`
```
RuntimeConfig
├─ server: ServerConfig
│  ├─ listen_addr: String                 // IP:port to bind
│  ├─ worker_threads: Option<u64>         // worker count override
│  └─ tls: Option<TlsConfig>
│     ├─ enabled: bool                    // enable TLS listener
│     ├─ cert_path: Option<String>        // certificate path
│     └─ key_path: Option<String>         // private key path
├─ telemetry: TelemetryConfig
│  ├─ level: Option<String>               // log level (e.g., info, debug)
│  ├─ pingora: Option<String>             // optional pingora log level
│  ├─ service_name: Option<String>        // service identifier
│  ├─ prometheus_addr: Option<String>     // metrics endpoint bind address
│  ├─ access_log: AccessLogConfig         // False | Stdout | File(path)
│  └─ tracing: Option<TracingConfig>
│     ├─ enabled: bool                    // tracing on/off
│     ├─ provider: String                 // tracing backend name
│     └─ sampling_rate: f64               // sampling rate (0.0–1.0)
├─ upstreams: Vec<Upstream>
│  ├─ name: String                        // cluster name
│  ├─ load_balancer: LoadBalancer         // RoundRobin | Random
│  ├─ http_version: HttpVersion           // H1 | H2 | H2H1
│  ├─ connection_pool: ConnectionPoolConfig
│  │  ├─ idle_timeout_secs: u64           // idle keepalive timeout
│  │  └─ connection_timeout_secs: u64     // connect timeout
│  ├─ tls: Option<UpstreamTlsConfig>
│  │  ├─ enabled: bool                    // enable upstream TLS
│  │  ├─ verify_hostname: bool            // enforce hostname verification
│  │  ├─ verify_cert: bool                // enforce certificate validation
│  │  └─ sni: Option<String>              // explicit SNI override
│  └─ endpoints: Vec<Endpoint>
│     ├─ ip: String                       // backend IP/hostname
│     ├─ port: u16                        // backend port
│     └─ weight: u32                      // load-balancing weight
└─ routes: Vec<VirtualHost>
   ├─ host: String                        // vhost match (e.g., example.com or *)
   └─ paths: Vec<Route>
      ├─ match_type: MatchType            // Prefix | Exact | Regex
      ├─ path: String                     // path pattern per match_type
      ├─ timeout_ms: Option<u64>          // per-route timeout
      ├─ retry_policy: Option<RetryPolicy>
      │  ├─ attempts: u32                 // retry attempts
      │  ├─ per_try_timeout_ms: u64       // timeout per try
      │  └─ retry_on: Vec<String>         // retry conditions
      ├─ request_headers: Option<HeaderOperations>
      │  ├─ add: Vec<(String, String)>    // headers to add
      │  └─ remove: Vec<String>           // headers to remove
      ├─ response_headers: Option<HeaderOperations>
      │  ├─ add: Vec<(String, String)>    // headers to add
      │  └─ remove: Vec<String>           // headers to remove
      ├─ destinations: Vec<WeightedDestination>
      │  ├─ upstream: String              // target upstream name
      │  └─ weight: u32                   // destination weight
      └─ compiled_regex: Option<regex::Regex>  // precompiled regex; runtime only
```

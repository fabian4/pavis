# Roadmap

> **Reference:** [Architecture.md](./Architecture.md) for technical details on components and protocols.

## Overview

| Phase | Focus                                                    | Status |
| :---: | -------------------------------------------------------- | :----: |
| 1     | Foundation (Pingora proxy)                               | ✅      |
| 2     | Protocol (`.pvs` format, `pavis-core`, `pavctl`)         | 🚧      |
| 3     | Long Polling (dynamic config updates)                    | 🚧      |
| 4     | Modular Ingestion (ingest + codec + relay)               | ⏸️      |
| 5     | Traffic Management (retries, timeouts, load balancing)   | ⏸️      |
| 6     | Security (mTLS, RBAC)                                    | ⏳      |
| 7     | Observability (metrics, tracing, logging)                | ⏳      |
| 8     | Operations (health checks, graceful shutdown)            | ⏳      |
| 9     | Advanced Features (rate limiting, fault injection, WASM) | ⏳      |
| 10    | Kubernetes Integration (operator, sidecar injection)     | ⏳      |

**Legend:** 🚧 In Progress · ⏳ Planned · ✅ Complete · ⏸️ Deferred

---

## Strategic Focus (Iron Triangle)

The roadmap is now centered on a three-part “iron triangle” that determines system viability.
Phase 4 and Phase 5 are intentionally deferred (not abandoned) to focus all effort here.

**A. Close the Loop – Dynamic Configuration**
- Scope: Complete Phase 3 client-side implementation and enable live, in-memory updates in `pavis-relay`.
- Goal: Running `pavis` instances detect and hot-reload config changes without traffic interruption.

**B. Enable Zero-Copy (mmap-based loading)**
- Scope: Complete Optimization Phase P2 tasks.
- Goal: Startup memory usage is minimal; config size primarily impacts page cache, not heap/RSS.

**C. Fix Concurrency Bottlenecks**
- Scope: Complete Optimization Phase P0 and P1 tasks.
- Goal: Sustain 10k concurrent connections without errors and outperform Envoy latency under comparable load.

---


---

## Phase 3: Long Polling (Iron Triangle) 🚧

**Goal:** Dynamic configuration updates via HTTP long polling.

**`pavis-relay`** (Server)
- [x] HTTP server setup (Axum)
  - [x] `GET /v1/config` - long-poll config fetch
  - [x] `GET /v1/status` - relay status/health
  - [x] `POST /v1/publish` - publish new `.pvs`
  - [x] `GET /v1/artifacts/{version}` - fetch specific version (optional)
  - [x] `GET /v1/metrics` - Prometheus metrics (optional)
- [x] Long polling implementation
  - [x] Accept `X-Pavis-Version` header
  - [x] Hold connection when client is up-to-date (configurable timeout, default 60s)
  - [x] Respond immediately on config change
  - [x] Handle multiple concurrent clients
- [ ] Response headers
  - [x] `X-Pavis-Version` - current version number
  - [x] `X-Pavis-Checksum` - sha256 payload checksum
  - [x] `X-Pavis-Checksum-Alg` - checksum algorithm label
  - [ ] `X-Pavis-Generated-At` - timestamp of config generation
- [x] Config storage
  - [x] In-memory config cache
  - [ ] File watcher for local `.pvs` changes
  - [ ] Version increment on change
- [x] Config history (unbounded; pruning TBD)

**`pavis-relay`** (Config Surface by Function)
- [ ] Identity metadata: identity.cluster, identity.instance_id
- [ ] HTTP/admin binding: http.admin_bind
- [ ] Storage backend: storage.type
- [ ] Artifact naming/paths: artifact.name, artifact.pvs_filename, artifact.artifacts_dir
- [ ] Artifact limits: artifact.limits.max_pvs_bytes, artifact.limits.max_routes
- [ ] Pipeline source ID: pipeline.source_id
- [ ] Ingest selection: pipeline.ingest.source.kind, pipeline.ingest.source.config.path
- [ ] Ingest mode tuning: pipeline.ingest.mode.kind, pipeline.ingest.mode.config.debounce_ms
- [ ] Codec selection: pipeline.codec.kind
- [ ] Codec options: pipeline.codec.options.strict_unknown_fields
- [ ] Versioning strategy: pipeline.execution.versioning.scheme, pipeline.execution.versioning.state_file
- [ ] Publish durability: pipeline.execution.publish.atomic_write, pipeline.execution.publish.fsync
- [x] Long-poll header override: distribution.long_poll.headers.algorithm
- [ ] Long-poll timeouts: distribution.long_poll.timeouts.hold_seconds, distribution.long_poll.timeouts.idle_seconds
- [ ] Direct fetch enable: distribution.direct_fetch.enabled
- [ ] Security auth: security.auth.mode, security.auth.bearer.token
- [ ] Logging: logging.level, logging.access_log
- [ ] Metrics bind: metrics.prometheus_bind

**Compatibility & Migration (Control Plane)**
- [ ] Relay accepts N-1 PVS and re-emits current version after core validation
- [ ] Record migration audit metadata (source version, target version)

**`pavctl`**

**`pavis`** (Client)
- [ ] Background config polling thread
  - [ ] Configurable poll interval and timeout
  - [ ] Exponential backoff on failures
  - [ ] Jitter to prevent thundering herd
  - [ ] Multi-source failover (primary/secondary xDS servers)
- [ ] Config hot reload
  - [ ] Atomic config swap (`ArcSwap`)
  - [ ] Validate new config before swap
  - [ ] Rollback on validation failure
  - [ ] Config diff logging
- [ ] Crash-loop protection
  - [ ] Persist config to disk (`/etc/pavis/config.pvs`)
  - [ ] Load from disk if control plane unavailable
  - [ ] Track last successful config timestamp
  - [ ] Bootstrap config for first start
- [ ] Metrics
  - [ ] `pavis_config_version` (gauge)
  - [ ] `pavis_config_last_reload_timestamp` (gauge)
  - [ ] `pavis_config_reload_total` (counter, success/failure labels)
  - [ ] `pavis_config_size_bytes` (gauge)

**E2E Tests**
- [ ] Config update triggers route change
- [ ] Long poll holds connection until update
- [ ] Checksum mismatch triggers retry
- [ ] Proxy continues serving during config reload
- [ ] Crash recovery loads config from disk
- [ ] Multiple proxies receive same update
- [ ] Exponential backoff on xDS server failure

---

## Architecture Alignment Checklist

- [x] Runtime (`pavis`) depends only on `pavis-core` and `pavis-pvs`
- [x] `pavis-pvs` performs binary integrity checks only (no semantic validation)
- [x] Codecs call `pavis-core::validate_runtime` after adaptation
- [ ] Relay (and later governor) owns migration and re-emits current-version PVS
- [ ] Compatibility fixtures (vN, vN-1) validated in CI for header/version compatibility

---

## Historical Phases (Context)

## Phase 1: Foundation ✅

**Goal:** Functional HTTP proxy with static configuration.

**Implementation**
- [x] Cloudflare Pingora integration (`ProxyHttp` trait)
- [x] Static upstream selection
- [x] CLI (`--config` flag)
- [x] Dockerfile, docker-compose
- [x] Basic routing (prefix, exact match)
- [x] Weighted traffic splitting (destination selection)
- [x] Request header manipulation (add/remove)
- [x] Round-robin load balancing
- [x] Response header manipulation
- [x] Regex route matching

**E2E Tests** (`tests/`)
- [x] Basic proxy startup and request forwarding
- [x] Multi-backend routing verification
- [x] Test prefix vs exact route matching
- [x] Test weighted traffic distribution (statistical)
- [x] Test header add/remove verification
- [x] Test 404 for unmatched routes
- [x] Test wildcard host matching
- [x] Test regex route matching

---

## Phase 2: Protocol 🚧

**Goal:** Define `.pvs` binary format and build tooling.

**`pavis-core`** (Library)
- [x] `RuntimeConfig` root struct with rkyv derivation
- [x] Basic types: `Upstream`, `Endpoint`, `VirtualHost`, `Route`
- [x] `LoadBalancer` enum (RoundRobin, Random)
- [x] `MatchType` enum (Prefix, Exact, Regex)
- [x] `HeaderOperations` for request/response manipulation
- [x] `WeightedDestination` for traffic splitting
- [x] Add schema migration strategy documentation

**`pavis-pvs`** (Protocol)
- [x] `PvsHeader`: Magic bytes (`PAVS`) + version (u32)
- [x] Header checksum verification + archive validation
- [x] `check_archived_root` regression tests for corrupted payloads
- [x] Version mismatch/unsupported algorithm coverage in tests
- [ ] Compatibility fixtures (vN, vN-1) header validation in CI

**`pavctl`** (Binary)
- [x] `gen` command: YAML → `.pvs`
  - [x] Parse YAML config with serde
  - [x] Convert to `pavis-core` structs
  - [x] Serialize with rkyv and write with header
  - [x] Validate references (routes → upstreams)
  - [x] Output file size
  - [ ] Output compression stats
- [x] `view` command: Debug binary files
  - [x] Display header (magic, version)
  - [x] Pretty-print config tree
  - [x] Show binary size and structure stats
  - [x] Hex dump mode for debugging
- [x] `check` command: Check YAML without compiling
- [x] `convert` command: `.pvs` → YAML (same version)
- [ ] `convert` command: Convert between versions (`--from`/`--to`)
- [ ] `apply` command: Push config to runtime (Phase 3)
- [ ] `status` command: View runtime health (Phase 8)
- [ ] `rollback` command: Revert config (Phase 3)

**`pavis`** (Binary)
- [x] Replace YAML loader with rkyv-based binary format
- [ ] Implement `mmap` + zero-copy access (see Optimization section)
- [x] Startup validation (magic bytes + version check)
- [x] Graceful error messages for invalid configs
- [x] Version mismatch handling (reject)
- [x] Remove semantic validation from `pavis-pvs`; ensure runtime only consumes already-validated configs

**E2E Tests**
- [x] Compile YAML → `.pvs` and verify binary structure
- [x] Load `.pvs` in proxy and forward traffic
- [x] Reject invalid magic bytes
- [x] Reject version mismatch
- [x] Inspect command output verification
- [x] Round-trip: YAML → `.pvs` → YAML (convert + validate)

---

## Deferred Phases (Paused / Deferred)

## Phase 4: Modular Ingestion ⏸️ Paused / Deferred

**Status:** Intentionally deprioritized to focus on the iron triangle.
This phase is deferred (not abandoned). No active milestones or deliverables are scheduled.

---

## Phase 5: Traffic Management ⏸️ Paused / Deferred

**Status:** Intentionally deprioritized to focus on the iron triangle.
This phase is deferred (not abandoned). No active milestones or deliverables are scheduled.

---

## Phase 6: Security ⏳

**Goal:** Secure service-to-service communication.

**mTLS** (`pavis-core` + `pavis`)
- [x] TLS configuration in `pavis-core`
  - [x] `TlsConfig` struct (cert, key; CA paths pending)
  - [ ] `TlsMode` enum (Disable, Permissive, Strict)
  - [ ] Cipher suite configuration
  - [ ] TLS version constraints (1.2, 1.3)
- [ ] TLS implementation in `pavis` (via Pingora/OpenSSL)
  - [x] Server-side TLS termination
  - [x] Client-side TLS origination
  - [ ] mTLS with client certificate validation
  - [ ] SNI-based routing
- [ ] Certificate management
  - [ ] File-based certificates
  - [ ] SDS integration (from xDS)
  - [ ] Hot reload on certificate rotation
  - [ ] Certificate expiry monitoring and alerts
  - [ ] SPIFFE/SPIRE integration

**Authorization** (`pavis-core` + `pavis`)
- [ ] `AuthzPolicy` struct in `pavis-core`
  - [ ] Source principals (service accounts)
  - [ ] Allowed methods and paths
  - [ ] Deny rules
  - [ ] Namespace/workload selectors
- [ ] RBAC enforcement in `pavis`
  - [ ] Extract identity from client certificate
  - [ ] Evaluate policies per request
  - [ ] Deny-by-default mode
  - [ ] Audit logging for denied requests

**JWT Validation** (`pavis-core` + `pavis`)
- [ ] `JwtPolicy` struct in `pavis-core`
  - [ ] Issuer validation
  - [ ] Audience validation
  - [ ] JWKS URI for key fetching
- [ ] JWT enforcement in `pavis`
  - [ ] Extract and validate JWT from headers
  - [ ] Cache JWKS with refresh
  - [ ] Claims extraction for routing

**E2E Tests**
- [x] TLS termination with valid cert
- [ ] Reject invalid client certificate
- [ ] mTLS handshake between services
- [ ] Certificate hot reload without downtime
- [ ] RBAC allows authorized request
- [ ] RBAC denies unauthorized request
- [ ] JWT validation accepts valid token
- [ ] JWT validation rejects expired token

---

## Phase 7: Observability ⏳

**Goal:** Full visibility into proxy behavior.

**Metrics** (`pavis`)
- [ ] Prometheus exporter endpoint (`/metrics`)
- [ ] Request metrics
  - [ ] `pavis_requests_total` (method, path, status, upstream)
  - [ ] `pavis_request_duration_seconds` (histogram)
  - [ ] `pavis_request_size_bytes` (histogram)
  - [ ] `pavis_response_size_bytes` (histogram)
- [ ] Connection metrics
  - [ ] `pavis_connections_active` (gauge)
  - [ ] `pavis_connections_total` (counter)
- [ ] Upstream metrics
  - [ ] `pavis_upstream_requests_total` (upstream, status)
  - [ ] `pavis_upstream_request_duration_seconds`
  - [ ] `pavis_upstream_connections_active`
  - [ ] `pavis_upstream_circuit_breaker_state`
  - [ ] `pavis_upstream_healthy_endpoints` (gauge)
- [ ] System metrics
  - [ ] `pavis_memory_bytes` (gauge)
  - [ ] `pavis_cpu_seconds_total` (counter)
  - [ ] `pavis_file_descriptors` (gauge)
  - [ ] Honor telemetry `prometheus_addr` config for metrics binding
  - [ ] Honor telemetry `service_name` config for metrics labeling

**Distributed Tracing** (`pavis`)
- [ ] OpenTelemetry integration
  - [ ] Trace context propagation (W3C, B3, Jaeger)
  - [ ] Span creation for requests
  - [ ] Configurable sampling rate
  - [ ] Parent-based sampling
- [ ] Trace export
  - [ ] OTLP exporter (gRPC/HTTP)
  - [ ] Jaeger exporter
  - [ ] Zipkin exporter
- [ ] Span attributes
  - [ ] HTTP method, path, status
  - [ ] Upstream name and address
  - [ ] Error details
  - [ ] Honor telemetry `tracing` config for tracing setup

**Access Logging** (`pavis`)
- [ ] Configurable log format (JSON, text, custom template)
- [ ] Log fields
  - [ ] Timestamp, method, path, status, duration
  - [ ] Client IP, upstream address
  - [ ] Request/response headers (configurable)
  - [ ] Trace ID, span ID
  - [ ] Bytes sent/received
- [ ] Log destinations
  - [ ] Stdout/stderr
  - [ ] File with rotation
  - [ ] Async buffered writes
  - [ ] Syslog

**E2E Tests**
- [ ] `/metrics` endpoint returns Prometheus format
- [ ] Request counter increments on traffic
- [ ] Histogram buckets populated correctly
- [ ] Trace ID propagated to upstream
- [ ] Trace appears in Jaeger/Zipkin
- [ ] Access log contains expected fields
- [ ] Log rotation works under load

---

## Phase 8: Operations ⏳

**Goal:** Production-ready operational features.

**Health Checks** (`pavis-core` + `pavis`)
- [ ] `HealthCheck` struct in `pavis-core`
  - [ ] `path` - HTTP path to check
  - [ ] `interval` - check frequency
  - [ ] `timeout` - per-check timeout
  - [ ] `healthy_threshold` - successes to mark healthy
  - [ ] `unhealthy_threshold` - failures to mark unhealthy
  - [ ] `expected_statuses` - valid response codes
- [ ] Active health checking in `pavis`
  - [ ] Background health check tasks per upstream
  - [ ] Remove unhealthy endpoints from rotation
  - [ ] Re-add on recovery
  - [ ] Health check connection reuse
- [ ] Passive health checking (outlier detection)
  - [ ] Track consecutive failures
  - [ ] Eject endpoints temporarily
  - [ ] Success rate ejection
  - [ ] Configurable ejection time
  - [ ] Honor upstream `health_check` config in runtime behavior

**Graceful Shutdown** (`pavis`)
- [ ] SIGTERM handling
- [ ] Drain existing connections (configurable timeout)
- [ ] Stop accepting new connections
- [ ] Health endpoint returns unhealthy during drain
- [ ] Wait for in-flight requests to complete

**Admin Interface** (`pavis`)
- [ ] Admin API (separate port)
  - [ ] `GET /config` - current config dump
  - [ ] `GET /clusters` - upstream status
  - [ ] `GET /stats` - internal statistics
  - [ ] `POST /drain` - trigger drain mode
  - [ ] `POST /logging` - change log level at runtime
- [ ] Debug endpoints
  - [ ] `GET /memory` - memory usage
  - [ ] `GET /connections` - active connections
  - [ ] `GET /certs` - certificate info and expiry

**`pavctl`** (Debugging)
- [ ] `status` command - query running proxy health and version
- [ ] `logs` command - stream proxy logs
- [ ] `visualize` command - render logical configuration structure
- [ ] `simulate` command - predict routing behavior for a config payload
- [ ] `config-diff` command - compare two `.pvs` files
- [ ] `traffic-tap` command - capture live traffic (development only)
- [ ] `cert-info` command - display certificate details

**E2E Tests**
- [ ] Unhealthy endpoint removed from rotation
- [ ] Endpoint recovers and rejoins pool
- [ ] Outlier detection ejects failing endpoint
- [ ] SIGTERM triggers graceful drain
- [ ] In-flight requests complete during drain
- [ ] Admin `/clusters` shows endpoint status
- [ ] Admin `/drain` stops new connections

---

## Phase 9: Advanced Features ⏳

**Goal:** Extended functionality for complex use cases.

**Rate Limiting** (`pavis-core` + `pavis`)
- [ ] `RateLimitPolicy` struct in `pavis-core`
  - [ ] Requests per second/minute/hour
  - [ ] Burst size
  - [ ] Key extraction (IP, header, path)
- [ ] Local rate limiting in `pavis`
  - [ ] Token bucket algorithm
  - [ ] Sliding window counter
  - [ ] Per-route and global limits
- [ ] Distributed rate limiting
  - [ ] Redis backend
  - [ ] Rate limit headers (`X-RateLimit-*`)

**Fault Injection** (`pavis-core` + `pavis`)
- [ ] `FaultInjection` struct in `pavis-core`
  - [ ] Delay injection (fixed, percentage)
  - [ ] Abort injection (HTTP status, percentage)
- [ ] Fault injection in `pavis`
  - [ ] Header-triggered faults (for testing)
  - [ ] Configurable fault targets (route, upstream)

**Request/Response Transformation**
- [ ] URL rewriting (path prefix, regex)
- [ ] Host header rewriting
- [ ] Response header manipulation
- [ ] Body transformation (future - requires buffering)

**Protocol Support**
- [ ] HTTP/2 upstream connections
- [ ] gRPC proxying
  - [ ] gRPC-specific health checks
  - [ ] gRPC status code handling
- [ ] WebSocket proxying
  - [ ] Connection upgrade handling
  - [ ] WebSocket-specific timeouts

**WASM Extensibility** (Future)
- [ ] WASM plugin loading
- [ ] Plugin API (request/response filters)
- [ ] Plugin marketplace/registry

**E2E Tests**
- [ ] Rate limit returns 429 when exceeded
- [ ] Rate limit headers present in response
- [ ] Fault injection adds delay
- [ ] Fault injection returns configured status
- [ ] URL rewrite changes path to upstream
- [ ] gRPC request proxied successfully
- [ ] WebSocket upgrade works

---

## Phase 10: Kubernetes Integration ⏳

**Goal:** Native Kubernetes deployment and management.

**Sidecar Injection**
- [ ] Mutating webhook for automatic injection
- [ ] Init container for iptables setup
- [ ] Configurable injection rules (namespace, labels)
- [ ] Resource limit configuration

**Pavis Operator** (`pavis-operator`)
- [ ] Custom Resource Definitions
  - [ ] `PavisConfig` - proxy configuration
  - [ ] `PavisGateway` - ingress gateway
  - [ ] `PavisPolicy` - traffic policies
- [ ] Controller implementation
  - [ ] Watch Kubernetes services
  - [ ] Generate pavis-core configs
- [ ] Integration with Gateway API
  - [ ] `HTTPRoute` support
  - [ ] `GRPCRoute` support

**Helm Chart**
- [ ] Pavis control plane deployment
- [ ] Configurable values
- [ ] Prometheus ServiceMonitor
- [ ] Grafana dashboards

**CNI Plugin** (Optional)
- [ ] Traffic interception without iptables
- [ ] eBPF-based redirection (future)

**E2E Tests** (requires Kubernetes cluster)
- [ ] Sidecar auto-injected into pod
- [ ] Traffic intercepted by sidecar
- [ ] Service discovery updates endpoints
- [ ] HTTPRoute creates correct config
- [ ] Helm install deploys control plane
- [ ] Upgrade preserves traffic

---

## Optimization & Stability (Immediate Priority) 🚧

**Goal:** Stabilize performance under high concurrency and reduce error rates.

**1. Upstream Concurrency Limits (P0)**
- **Goal:** Prevent upstream saturation under high concurrency.
- **Tasks:**
  - [ ] Add a per-upstream limit for in-flight requests / active connections
  - [ ] Enforce backpressure when the limit is reached (queue or fail fast)
  - [ ] Expose the limit as a configurable parameter

**2. Improve Upstream Connection Reuse (P0)**
- **Goal:** Reduce connection churn and upstream accept pressure.
- **Tasks:**
  - [ ] Enable and tune upstream keepalive by default
  - [ ] Maintain a reusable connection pool per upstream
  - [ ] Avoid creating new TCP connections when idle connections are available

**3. Enable Limited Retry for Idempotent Requests (P1)**
- **Goal:** Reduce transient upstream failures surfacing as 502 errors.
- **Tasks:**
  - [ ] Enable retry for idempotent methods (e.g. GET)
  - [ ] Retry only on upstream reset / early close errors
  - [ ] Limit retries to a single attempt to avoid traffic amplification

**4. Throttle or Downgrade Error Logging (P1)**
- **Goal:** Prevent error log storms from becoming a performance bottleneck.
- **Tasks:**
  - [ ] Add rate limiting for repetitive upstream error logs
  - [ ] Downgrade expected upstream errors to debug level in benchmark mode
  - [ ] Optionally aggregate identical errors over a time window

**5. Zero-Copy Configuration Access (P2)**
- **Goal:** Fully realize the performance benefits of `rkyv` and `mmap`.
- **Tasks:**
  - [ ] Refactor `pavis` runtime to operate on `ArchivedRuntimeConfig` instead of owned DTOs
  - [ ] Remove eager deserialization from `crates/pavis/src/load/fs.rs`
  - [ ] Implement lazy regex compilation for archived routes

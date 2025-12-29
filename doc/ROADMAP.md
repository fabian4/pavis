# Roadmap

> **Reference:** [Architecture.md](./Architecture.md) for technical details on components and protocols.

## Overview

| Phase | Focus | Status |
|:-----:|-------|:------:|
| 1 | Foundation (Pingora proxy) | ✅ |
| 2 | Protocol (`.pvs` format, `pavis-core`, `pavctl`) | 🚧 |
| 3 | Long Polling (dynamic config updates) | ⏳ |
| 4 | Modular Ingestion (ingest + codec + relay) | ⏳ |
| 5 | Traffic Management (retries, timeouts, load balancing) | ⏳ |
| 6 | Security (mTLS, RBAC) | ⏳ |
| 7 | Observability (metrics, tracing, logging) | ⏳ |
| 8 | Operations (health checks, graceful shutdown) | ⏳ |
| 9 | Advanced Features (rate limiting, fault injection, WASM) | ⏳ |
| 10 | Kubernetes Integration (operator, sidecar injection) | ⏳ |

**Legend:** 🚧 In Progress · ⏳ Planned · ✅ Complete

---

## Architecture Alignment Checklist

- [ ] Runtime (`pavis`) depends only on `pavis-core` and `pavis-pvs`
- [ ] `pavis-pvs` performs binary integrity checks only (no semantic validation)
- [ ] Codecs call `pavis-core::validate_runtime` after adaptation
- [ ] Relay (and later governor) owns migration and re-emits current-version PVS
- [ ] Compatibility fixtures (vN, vN-1) validated in CI for header/version compatibility

---

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
- [ ] `check_archived_root` regression tests for corrupted payloads
- [ ] Version mismatch/unsupported algorithm coverage in tests
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
  - [ ] Show binary size and structure stats
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
- [x] Version mismatch handling (reject vs warn)
- [ ] Remove semantic validation from `pavis-pvs`; ensure runtime only consumes already-validated configs

**E2E Tests**
- [x] Compile YAML → `.pvs` and verify binary structure
- [x] Load `.pvs` in proxy and forward traffic
- [x] Reject invalid magic bytes
- [ ] Reject version mismatch
- [ ] Inspect command output verification
- [x] Round-trip: YAML → `.pvs` → YAML (convert + validate)

---

## Phase 3: Long Polling ⏳

**Goal:** Dynamic configuration updates via HTTP long polling.

**`pavis-relay`** (Server)
- [ ] HTTP server setup (Axum)
  - [ ] `GET /v1/config` - fetch current config
  - [ ] `GET /v1/config/version` - fetch version only
  - [ ] `GET /health` - liveness probe
  - [ ] `GET /ready` - readiness probe
- [ ] Long polling implementation
  - [ ] Accept `X-Pavis-Version` header
  - [ ] Hold connection when client is up-to-date (configurable timeout, default 60s)
  - [ ] Respond immediately on config change
  - [ ] Handle multiple concurrent clients
- [ ] Response headers
  - [ ] `X-Pavis-Version` - current version number
  - [ ] `X-Pavis-Checksum` - xxhash for integrity verification
  - [ ] `X-Pavis-Generated-At` - timestamp of config generation
- [ ] Config storage
  - [ ] In-memory config cache
  - [ ] File watcher for local `.pvs` changes
  - [ ] Version increment on change
  - [ ] Config history (last N versions)

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

## Phase 4: Modular Ingestion ⏳

**Goal:** Standardize the configuration pipeline with ingest sources, codecs, and relay orchestration.

**`pavis-relay`** (Orchestrator)
- [ ] Registry system for one active ingest/codec pair
- [ ] State reconciliation engine
- [ ] NACK feedback loop for validation failures
- [ ] Version management and PVS emission

**Ingest** (Source Connectivity)
- [ ] `pavis-ingest-file`: Local directory/file watcher
- [ ] `pavis-ingest-istio`: xDS gRPC client
- [ ] `pavis-ingest-kuma`: xDS gRPC client (reusing xDS ingest)
- [ ] `pavis-ingest-k8s`: Kubernetes API watcher

**Codecs** (Protocol Translation)
- [ ] `pavis-codec-xds`: Envoy Protobuf → `RuntimeConfig`
- [x] `pavis-codec-yaml`: YAML DTO → `RuntimeConfig`
- [ ] `pavis-codec-crd`: K8s Gateway API → `RuntimeConfig`

**E2E Tests**
- [ ] Source switch: Verify proxy updates when relay switches ingest sources
- [ ] Protocol reuse: Verify same xDS codec works for both Istio and Kuma ingest
- [ ] Conflict gate: Verify relay prevents concurrent source definitions

---

## Phase 5: Traffic Management ⏳

**Goal:** Advanced traffic control features.

**Retries** (`pavis-core` + `pavis`)
- [x] `RetryPolicy` struct in `pavis-core`
  - [x] `attempts` - max retry count
  - [x] `per_try_timeout` - timeout per attempt
  - [x] `retry_on` - conditions (5xx, connect-failure, reset, etc.)
  - [ ] `retry_back_off` - base interval and max interval
  - [ ] `retriable_headers` - retry on specific response headers
- [ ] Retry implementation in `pavis`
  - [ ] Retry on configured status codes
  - [ ] Retry on connection failures
  - [ ] Respect retry budget (prevent retry storms)
  - [ ] Hedged requests (speculative retries)

**Timeouts** (`pavis-core` + `pavis`)
- [ ] `TimeoutPolicy` struct in `pavis-core`
  - [ ] `request_timeout` - total request timeout
  - [ ] `idle_timeout` - connection idle timeout
  - [ ] `connect_timeout` - upstream connect timeout
  - [ ] `stream_idle_timeout` - for long-lived streams
- [ ] Timeout enforcement in `pavis`
  - [ ] Per-route timeout overrides
  - [ ] Timeout headers (`x-envoy-upstream-rq-timeout-ms`)

**Circuit Breaking** (`pavis-core` + `pavis`)
- [ ] `CircuitBreaker` struct in `pavis-core`
  - [ ] `max_connections` - per upstream
  - [ ] `max_pending_requests` - queue limit
  - [ ] `max_requests` - concurrent requests limit
  - [ ] `max_retries` - concurrent retries limit
- [ ] Circuit breaker state machine in `pavis`
  - [ ] Closed → Open on threshold breach
  - [ ] Open → Half-Open after timeout
  - [ ] Half-Open → Closed on success
  - [ ] Circuit breaker metrics and events

**Load Balancing** (`pavis-core` + `pavis`)
- [ ] Additional algorithms in `LoadBalancer` enum
  - [ ] `LeastConnections`
  - [ ] `WeightedRoundRobin`
  - [ ] `ConsistentHash` (header, cookie, IP)
  - [ ] `Maglev`
  - [ ] `P2C` (Power of Two Choices)
- [ ] Locality-aware routing
  - [ ] Zone preference
  - [ ] Failover to other zones
  - [ ] Priority levels
- [ ] Slow start mode
  - [ ] Gradually increase traffic to new endpoints

**Traffic Splitting** (`pavis`)
- [x] Weighted routing (canary deployments)
- [ ] Header-based routing
- [ ] Cookie-based routing (sticky sessions)
- [ ] Mirror/shadow traffic
  - [ ] Fire-and-forget mirroring
  - [ ] Configurable mirror percentage

**E2E Tests**
- [ ] Retry succeeds after transient 503
- [ ] Retry exhaustion returns final error
- [ ] Request timeout returns 504
- [ ] Circuit breaker opens after failures
- [ ] Circuit breaker closes after recovery
- [ ] Round-robin distributes evenly
- [ ] Consistent hash routes same key to same backend
- [ ] Canary weight splits traffic correctly
- [ ] Mirror sends copy without affecting response

---

## Phase 6: Security ⏳

**Goal:** Secure service-to-service communication.

**mTLS** (`pavis-core` + `pavis`)
- [ ] TLS configuration in `pavis-core`
  - [ ] `TlsConfig` struct (cert, key, CA paths)
  - [ ] `TlsMode` enum (Disable, Permissive, Strict)
  - [ ] Cipher suite configuration
  - [ ] TLS version constraints (1.2, 1.3)
- [ ] TLS implementation in `pavis` (using `rustls`)
  - [ ] Server-side TLS termination
  - [ ] Client-side TLS origination
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
- [ ] TLS termination with valid cert
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

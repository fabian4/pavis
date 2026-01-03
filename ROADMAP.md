# Pavis Roadmap

**Summary**
- **Total**: 37/81
- **Foundation**: 18/23
- **Protocol**: 12/13
- **Security**: 0/4
- **Observability**: 0/4
- **Operations**: 7/21
- **Advanced Features**: 0/14
- **Kubernetes Integration**: 0/2

> **Reference:** [Architecture.md](./Architecture.md) for technical details on components and protocols.

---

## Foundation

### Phase 1: Core Setup & Infrastructure
- [x] [MUST] **Cloudflare Pingora integration**: Implementation of the `ProxyHttp` trait.
- [x] [MUST] **Upstreams**: Static IP-based selection (DNS unsupported).
- [x] [MUST] **Routing**: Basic routing (prefix, exact, regex, wildcard); Forwarding only.
- [x] [MUST] **Traffic**: Weighted splitting and Round-robin load balancing.
- [x] [MUST] **Headers**: Request/Response manipulation (Insert/Overwrite behavior).
- [x] [MUST] **Listener**: Single-listener support with file-based TLS.
- [x] [MUST] **CLI & Docker**: Support for `--config` and containerization.
- [x] [MUST] Runtime (`pavis`) dependency on `pavis-core` and `pavis-pvs` only.
- [x] [MUST] `pavis-pvs` binary integrity checks (no semantic validation).
- [x] [MUST] Codec validation using `pavis-core::validate_runtime` after adaptation.
- [ ] [MUST] Relay (and later Governor) ownership of migration and current-version PVS emission.
- [ ] [MUST] CI validation of compatibility fixtures (vN, vN-1) for header/version compatibility.
- [ ] [MUST] **Unit Testing**: 100% coverage of logic branches for new features.
- [ ] [MUST] **Integration Testing**: Verify `RuntimeConfig` changes propagate to internal state without restarts.
- [ ] [MUST] **End-to-End Testing**: Full `pavis` binary against real network targets (Docker/Kind).

### Phase 3: Dynamic Configuration Surface
- [x] [MUST] **Close the Loop**: Enable live, hitless configuration updates (Ingest -> Codec -> Long-Poll -> Hot Reload).
- [ ] [MUST] **Enable Zero-Copy**: mmap-based loading using `ArchivedRuntimeConfig`.
- [x] [MUST] **Identity & Bindings**: `identity.*`, `http.admin_bind`, `metrics.prometheus_bind`.
- [x] [MUST] **Artifacts**: Naming, paths, limits (`max_pvs_bytes`, `max_routes`), and storage backend.
- [x] [MUST] **Pipeline**: Source ID, ingest settings, codec selection, and strictness options.
- [x] [MUST] **Execution**: Versioning scheme and atomic write/fsync durability.
- [x] [MUST] **Distribution**: Long-poll tuning (headers, timeouts) and direct fetch toggles.

---

## Protocol

### Phase 2: Binary Format & Tooling
- [x] [MUST] **`pavis-core`**: `RuntimeConfig` schema with `rkyv` derivation.
- [x] [MUST] **`pavis-pvs`**: `PvsHeader` with Magic Bytes (`PAVS`) and checksum verification.
- [x] [MUST] **Binary Integrity**: Validation logic in the protocol layer.
- [x] [MUST] **`pavctl gen`**: Compile YAML to `.pvs` with validation.
- [x] [MUST] **`pavctl view`**: Inspect/debug binary files.
- [x] [MUST] **`pavctl check`**: Validate YAML without compiling.
- [x] [MUST] **`pavctl convert`**: Reverse `.pvs` to YAML.
- [x] [MUST] **Runtime Loading**: Load `rkyv`-based binary format with version checks.
- [x] [MUST] **Error Handling**: Graceful rejection of invalid/mismatched configs.

### Phase 3: Long Polling API
- [x] [MUST] **Endpoints**: Implement `GET /v1/config`, `GET /v1/status`, `POST /v1/publish`, and `GET /v1/artifacts/{version}`.
- [x] [MUST] **Long Polling Protocol**: 60s hold until update, `X-Pavis-Version` mismatch handling, and concurrent client support.
- [x] [MUST] **Integrity Headers**: `X-Pavis-Version`, `X-Pavis-Checksum`, and `X-Pavis-Checksum-Alg`.
- [ ] [SHOULD] **Traceability**: Add `X-Pavis-Generated-At` header.

---

## Security

### Phase 4: Core TLS Enhancements
- [ ] [SHOULD] **TLS Enhancements**: Support inline certificates (SDS-style) and multiple certs per listener (SNI).

### Phase 6: Secure Communication
- [ ] [MUST] **mTLS**: Client/Server TLS, certificate management, and SPIFFE/SPIRE integration.
- [ ] [MUST] **Authorization**: RBAC policies, deny-by-default, and audit logging.
- [ ] [SHOULD] **Identity**: JWT validation and JWKS caching.

---

## Observability

### Phase 3: Runtime Visibility
- [ ] [SHOULD] **Metrics**: Track config version, reload counts (success/fail), and payload size.

### Phase 7: System-Wide Observability
- [ ] [MUST] **Metrics**: Prometheus endpoint with request, connection, and upstream stats.
- [ ] [MUST] **Access Logs**: Configurable formats (JSON/Text) and destinations.
- [ ] [SHOULD] **Tracing**: OpenTelemetry integration (OTLP/Jaeger/Zipkin).

---

## Operations

### Phase 3: Relay & Data Plane Operations
- [x] [MUST] **In-Memory Cache**: Fast access to current config and history in `pavis-relay`.
- [x] [MUST] **Pipeline Ingestion**: `pavis-ingest-file` with debounced watching and automatic versioning.
- [x] [MUST] **Durability**: Atomic Last-Known-Good (LKG) persistence with `fsync` safety.
- [x] [MUST] **Polling Thread**: Periodic polling with exponential backoff and jitter in `pavis`.
- [x] [MUST] **Hot Reload**: Atomic `ArcSwap` of runtime config with pre-swap validation and rollback.
- [ ] [SHOULD] **Tuning**: Configurable poll intervals, timeouts, and multi-source failover.
- [ ] [SHOULD] **Visibility**: Config diff logging on update.
- [x] [MUST] **Persistence**: Save last-known-good config to disk (`/etc/pavis/config.pvs`).
- [x] [MUST] **Recovery**: Boot from disk if control plane is unreachable.
- [ ] [SHOULD] **Safety**: Track last successful reload timestamp for heuristic checks.
- [x] [MUST] Config update triggers route change.
- [x] [MUST] Long poll holds connection until update.
- [x] [MUST] Checksum mismatch triggers retry.
- [x] [MUST] Proxy continues serving during config reload.
- [x] [MUST] Crash recovery loads config from disk.
- [x] [MUST] Multiple proxies receive same update.
- [ ] [SHOULD] Exponential backoff on xDS server failure.

### Phase 4 & 8: Operational Lifecycle
- [ ] [NICE] **Modular Ingestion**: Control-plane pipeline migration (accept N-1, emit N).
- [ ] [MUST] **Health Checks**: Active/Passive checks and outlier detection.
- [ ] [MUST] **Lifecycle**: Graceful shutdown and connection draining.
- [ ] [SHOULD] **Admin API**: Runtime stats, config dumps, and log level changes.

---

## Advanced Features

### Phase 3: Optimization & Stability
- [ ] [MUST] **Concurrency Limits**: Enforce per-upstream limits on in-flight requests/connections.
- [ ] [MUST] **Connection Reuse**: Enable upstream keepalive and maintain a reusable connection pool.
- [ ] [SHOULD] **Idempotent Retries**: Limited retries for idempotent methods on transient errors.
- [ ] [SHOULD] **Log Throttling**: Rate-limit or aggregate repetitive upstream errors.
- [ ] [NICE] **Zero-Copy Access**: Refactor runtime to use `ArchivedRuntimeConfig` (mmap) via `rkyv`.
- [ ] [NICE] **Lazy Compilation**: Implement lazy regex compilation for archived routes.

### Phase 4 & 5: Core Expansion & Traffic
- [ ] [SHOULD] **Multi-Listener Support**: Support multiple bind addresses and protocols in a single runtime.
- [ ] [SHOULD] **DNS-Based Upstreams**: Hostname-based dynamic backends using `LOGICAL_DNS`.
- [ ] [SHOULD] **Advanced Route Actions**: `DirectResponse`, `Redirect`, `HostRewrite`, and `PathRewrite`.
- [ ] [NICE] **Traffic Management**: Request timeouts, retry policies, and circuit breakers.

### Phase 9: Extended Functionality
- [ ] [NICE] **Traffic Control**: Rate limiting (local/distributed) and Fault injection.
- [ ] [NICE] **Transforms**: URL rewriting and body transformation.
- [ ] [NICE] **Protocols**: gRPC, WebSocket, and HTTP/2 upstream.
- [ ] [NICE] **Extensibility**: WASM plugins.

---

## Kubernetes Integration

### Phase 10: Native Kubernetes
- [ ] [SHOULD] **Operator**: CRDs (`PavisConfig`, `PavisGateway`) and Controller.
- [ ] [NICE] **Deployment**: Sidecar injection and Helm charts.

# Pavis Feature Verification Report (Code-Based)

## Summary
- Total features audited: 28
- Status counts: 9 Implemented / 3 Partially Implemented / 3 Parsed but Ignored / 1 Blocked by Upstream / 12 Not Implemented
- High-risk mismatches: header/method routing advertised but absent; route-level retries/timeouts and upstream health/circuit knobs are accepted yet ignored; inbound mTLS and custom CA handling are blocked by Pingora, so enabling them today produces a false sense of security.

## 1. Traffic Management

### Feature: L7 Routing (Path/Header/Method)
- Claimed Status: ✅
- Actual Status: Partially Implemented
- Evidence: `crates\pavis-core\src\runtime\routing.rs` defines `Route` with only `PathMatch` variants; `crates\pavis\src\router\matcher.rs` performs host + path matching only.
- Notes: Host prefix/exact/regex routing works, but there is no logic inspecting HTTP methods or headers despite the claim.
- Verdict: Path routing only.

### Feature: Traffic Splitting (Canary)
- Claimed Status: ✅
- Actual Status: Implemented
- Evidence: `crates\pavis\src\proxy\service.rs` (`RouteAction::Forward` block around lines 571-596) randomly selects destinations according to configured weights.
- Notes: Weighted random selection occurs before upstream lookup; zero-weight sets short-circuit.
- Verdict: Matches claim.

### Feature: Header Manipulation
- Claimed Status: ✅
- Actual Status: Implemented
- Evidence: `crates\pavis\src\proxy\header_ops.rs` applies set/append/add/remove operations for both request and response flows; invoked from `Proxy::upstream_request_filter` and `Proxy::upstream_response_filter`.
- Notes: Invalid header names/values are dropped with warnings, ensuring deterministic behavior.
- Verdict: Matches claim.

### Feature: Redirect & DirectResponse
- Claimed Status: ✅
- Actual Status: Implemented
- Evidence: `crates\pavis\src\proxy\service.rs` handles `RouteAction::Redirect` (lines 599-611) and `RouteAction::Direct` (lines 612-627) by constructing Pingora responses and applying header policies.
- Notes: Redirect responses include `Location` and zero body; direct responses emit static text bodies.
- Verdict: Matches claim.

### Feature: Rewrite (Host/Path)
- Claimed Status: ✅
- Actual Status: Implemented
- Evidence: `calculate_path_rewrite` in `crates\pavis\src\proxy\service.rs` rewrites prefix/exact matches; host rewrite logic (lines 547-565) overwrites the Host header and SNI override.
- Notes: Regex routes skip rewrites with warnings; only literal host rewrites are supported as documented.
- Verdict: Matches claim.

### Feature: Retries & Timeouts
- Claimed Status: ⏳
- Actual Status: Parsed but Ignored
- Evidence: `crates\pavis-core\src\runtime\routing.rs` stores `timeout` and `retry` inside each `Route`, but no runtime code references `route.timeout` or `route.retry` (ripgrep under `crates\pavis` finds zero uses outside tests).
- Notes: Config accepts these fields, yet request handling never sets Pingora per-route deadlines or retry policies.
- Verdict: Configuration no-op today.

### Feature: Traffic Mirroring (Shadowing)
- Claimed Status: ⚠️
- Actual Status: Not Implemented
- Evidence: `RouteAction` enum (`crates\pavis-core\src\runtime\routing.rs`) contains only `Forward`, `Redirect`, and `Direct`; runtime switch over `RouteAction` lacks any mirror path.
- Notes: No duplicate downstream/upstream request wiring exists.
- Verdict: Unsupported.

### Feature: Global Rate Limiting
- Claimed Status: ❌
- Actual Status: Not Implemented
- Evidence: No rate-limit structs or logic exist in `pavis-core` or `crates\pavis` (ripgrep for `rate_limit` / `ratelimit` under `crates` returns nothing).
- Notes: Runtime never counts requests per policy or contacts external limiters.
- Verdict: Unsupported (matches "Dropped").

### Feature: Local Rate Limiting
- Claimed Status: ⚠️
- Actual Status: Not Implemented
- Evidence: Same absence of any local token-bucket or leaky-bucket implementation in runtime.
- Notes: Despite being deferred, no scaffolding exists.
- Verdict: Unsupported.

## 2. Security (Critical Path)

### Feature: Server TLS (Termination)
- Claimed Status: ✅
- Actual Status: Implemented
- Evidence: `crates\pavis\src\main.rs` configures Pingora listeners via `add_tls_with_settings`, loading a single cert/key per listener and honoring `ClientAuth` modes.
- Notes: TLS termination works for one identity per listener.
- Verdict: Matches claim.

### Feature: Upstream TLS (Origination)
- Claimed Status: ⚠️
- Actual Status: Partially Implemented
- Evidence: `Proxy::upstream_peer` (`crates\pavis\src\proxy\service.rs`) enables TLS, enforces `TlsVerify`, and tries to attach custom CA bundles, but the comment at lines 392-416 states Pingora’s rustls connector ignores `peer.options.ca`.
- Notes: TLS handshakes and verify toggles work, yet custom CA files are silently ineffective.
- Verdict: Functional with notable CA limitation.

### Feature: Inbound mTLS (Client Cert Validation)
- Claimed Status: ⚠️
- Actual Status: Blocked by Upstream Dependency
- Evidence: `configure_client_auth` (`crates\pavis\src\main.rs` lines 47-85) builds rustls verifiers but comments “// TODO: wire the verifier into Pingora once its Rustls settings expose a setter... PENDING: pingora#791”.
- Notes: Config allows `ClientAuth::Optional/Required`, but Pingora cannot currently enforce it; requests proceed without client cert checks.
- Verdict: Blocked until Pingora exposes verifier hooks.

### Feature: Outbound mTLS (Client Cert to Upstream)
- Claimed Status: ⚠️
- Actual Status: Partially Implemented
- Evidence: `crates\pavis\src\upstream.rs::Manager::new` loads client cert/key material and stores it per cluster; `Proxy::upstream_peer` (lines 364-385) assigns `peer.client_cert_key`, but the same CA limitation (lines 392-416) prevents custom trust anchors.
- Notes: Client certificates are sent, yet validating upstream certificates against user-provided CAs is ineffective under rustls.
- Verdict: Works for client-auth but not for custom verification chains.

### Feature: RBAC (Path/Method Auth)
- Claimed Status: ✅
- Actual Status: Implemented
- Evidence: `is_authorized` and call site in `Proxy::request_filter` (`crates\pavis\src\proxy\service.rs` lines 520-546) enforce `Principal` matches before routing.
- Notes: Default is deny-unless principal matches `Any`; tracing logs mark RBAC denials.
- Verdict: Matches claim.

### Feature: SNI Multi-Cert
- Claimed Status: ❌
- Actual Status: Not Implemented
- Evidence: `TlsConfig::Enabled` (`crates\pavis-core\src\runtime\server.rs`) supports exactly one `cert_path` and `key_path`; no SNI-based certificate map exists.
- Notes: Users must deploy separate listeners for multiple identities.
- Verdict: Unsupported (matches claim).

### Feature: External Auth (OIDC/OAuth)
- Claimed Status: ❌
- Actual Status: Not Implemented
- Evidence: Ripgrep over `crates\pavis*` finds no occurrences of “oidc” or “oauth”; runtime contains no filters, token validators, or ext-auth hooks.
- Notes: Requests pass directly to upstreams without auth delegation.
- Verdict: Unsupported (matches claim).

### Feature: WAF (ModSecurity)
- Claimed Status: ❌
- Actual Status: Not Implemented
- Evidence: No code references “modsecurity”, “waf”, or body inspection modules; only Pingora proxying is present.
- Notes: No inline inspection or rule engine exists.
- Verdict: Unsupported (matches claim).

## 3. Resilience

### Feature: Active Health Check
- Claimed Status: ⏳
- Actual Status: Parsed but Ignored
- Evidence: Serde config exposes `health_check` (`crates\pavis-codec-serde\src\config\types\upstreams.rs`), yet `to_runtime` in the same module never reads it and `pavis-core::Upstream` lacks a field.
- Notes: User-provided health checks are discarded during codec conversion, so no probes run.
- Verdict: Configuration no-op today.

### Feature: Outlier Detection (Passive)
- Claimed Status: ⏳
- Actual Status: Not Implemented
- Evidence: `crates\pavis\src\upstream\cluster.rs` maintains weights but never tracks failure counts or ejects endpoints; no logic references “outlier” or status buckets.
- Notes: All endpoints remain eligible regardless of 5xx streams.
- Verdict: Unsupported.

### Feature: Circuit Breaking
- Claimed Status: ⏳
- Actual Status: Parsed but Ignored
- Evidence: `pavis-core::Pool` carries `ConnectionLimit max`, but the runtime never references `pool.max` (ripgrep across `crates\pavis` finds zero matches) and Pingora peers are created without max-connections enforcement.
- Notes: Configured limits have no effect, exposing the proxy to unbounded upstream fan-out.
- Verdict: Configuration no-op today.

### Feature: Fault Injection
- Claimed Status: ⚠️
- Actual Status: Not Implemented
- Evidence: No modules introduce latency, abort, or rate override behaviors; `RouteAction` lacks any fault modes.
- Notes: Chaos testing must be done externally.
- Verdict: Unsupported.

## 4. Observability

### Feature: Prometheus Metrics
- Claimed Status: ✅
- Actual Status: Implemented
- Evidence: `crates\pavis\src\telemetry\metrics.rs` exposes a Prometheus exporter and `Proxy::request_filter` / `Proxy::logging` record counters, histograms, and inflight gauges via `MetricsHandle`.
- Notes: Metrics worker binds to user-configured addr and exports prefixed series.
- Verdict: Matches claim.

### Feature: Access Logs (JSON)
- Claimed Status: ✅
- Actual Status: Implemented
- Evidence: `crates\pavis\src\telemetry\access_log.rs` streams structured JSON entries over stdout/file, and `Proxy::logging` pushes entries asynchronously.
- Notes: Includes route pattern, upstream, bytes, request ID, and RBAC flags.
- Verdict: Matches claim.

### Feature: Distributed Tracing (OTLP)
- Claimed Status: ✅
- Actual Status: Implemented
- Evidence: `crates\pavis\src\telemetry\tracing.rs` starts an OTLP exporter with dynamic reload; `Proxy::request_filter` creates spans when tracing enabled and `upstream_request_filter` injects TraceContext headers.
- Notes: Sampling is controlled via config, and spans capture route/upstream labels.
- Verdict: Matches claim.

### Feature: Tap / Packet Capture
- Claimed Status: ❌
- Actual Status: Not Implemented
- Evidence: No tap/capture modules exist in runtime; only standard Pingora proxying is present.
- Notes: Users must rely on external tools (`tcpdump`, eBPF) as documented.
- Verdict: Unsupported (matches claim).

## 5. Extensibility

### Feature: Wasm Plugins
- Claimed Status: ❌
- Actual Status: Not Implemented
- Evidence: Searching `crates\pavis*` for “wasm” shows no runtime references; the dependency graph only brings wasm-bindgen transitively for unrelated crates.
- Notes: No plugin loader or sandbox exists.
- Verdict: Unsupported (matches claim).

### Feature: Lua Scripting
- Claimed Status: ❌
- Actual Status: Not Implemented
- Evidence: No references to “lua” under the runtime crates.
- Notes: No interpreter or hook points provided.
- Verdict: Unsupported (matches claim).

### Feature: gRPC Transcoding
- Claimed Status: ❌
- Actual Status: Not Implemented
- Evidence: Ripgrep for “transcod” yields nothing; there is no protobuf/grpc gateway layer in the runtime.
- Notes: Proxy forwards HTTP as-is.
- Verdict: Unsupported (matches claim).

## Appendix

- Features claimed but not enforced: L7 header/method routing, route retries/timeouts, upstream custom CA enforcement, Active Health Check, Circuit Breaking.
- Config accepted but ignored: `Route.timeout` / `Route.retry`; `Upstream.health_check`; `Upstream.pool.max`; inbound `ClientAuth::{Optional,Required}`; upstream `UpstreamCa::File` (due to Pingora rustls ignoring `peer.options.ca`).
- Blocked solely by upstream (Pingora): inbound mTLS verifier wiring; per-peer CA bundles for rustls connectors.
- Should be rejected during validation but currently are not: configs enabling client-auth on listeners, route retries/timeouts, upstream health checks, and circuit breaker limits all pass validation despite being inert, risking silent misconfiguration.

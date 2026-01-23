use crate::proxy::context::{RequestId, RouterContext, TracingSpan};
use crate::proxy::header_ops::{apply_request_headers, apply_response_headers};
use crate::retry::RetryContext;
use crate::state::RuntimeStateHandle;
use crate::telemetry::Telemetry;
use crate::upstream::cluster::{CircuitBreakerRejection, PoolRejection, UpstreamOutcome};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use http::Uri;
use pavis_core::{
    ConnectTimeout, Discovery, EndpointAddr, HeadersPolicy, Hostname, HttpVersion, PathMatch,
    RetryPolicy, RouteAction, Timeout, TryTimeout,
};
use pingora::ErrorType;
use pingora::http::RequestHeader;
use pingora::http::ResponseHeader;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use rand::Rng;
use rustls::RootCertStore;
use std::borrow::Cow;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub struct Proxy {
    pub state: Arc<RuntimeStateHandle>,
    pub telemetry: Arc<Telemetry>,
    pub ca_store: Arc<ArcSwap<RootCertStore>>,
}

impl Proxy {}

struct HeaderInjector<'a>(&'a mut RequestHeader);

impl opentelemetry::propagation::Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(value)) = (
            http::header::HeaderName::try_from(key),
            value.parse::<http::header::HeaderValue>(),
        ) {
            let _ = self.0.insert_header(name, value);
        }
    }
}

static CLOCK_UNDERFLOW_WARNED: AtomicBool = AtomicBool::new(false);

fn request_id_timestamp(now: std::time::SystemTime) -> u128 {
    match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos(),
        Err(err) => {
            if !CLOCK_UNDERFLOW_WARNED.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    error = %err,
                    "System clock is before UNIX_EPOCH; using 0 for request id timestamp"
                );
            }
            0
        }
    }
}

fn generate_request_id() -> RequestId {
    let now = request_id_timestamp(std::time::SystemTime::now());
    let random_val: u32 = rand::rng().random();
    RequestId::from_parts(now, random_val)
}

fn apply_route_headers(ctx: &mut RouterContext, route: &pavis_core::Route) {
    ctx.request_headers = route.request_headers.clone();
    ctx.response_headers = route.response_headers.clone();
    ctx.route_timeout = route.timeout;
    ctx.retry_policy = route.retry.clone();
    ctx.retry_attempts = 0;
}

fn core_duration_to_std(duration: &pavis_core::Duration) -> Duration {
    Duration::from_millis(duration.0.get() as u64)
}

fn resolve_route_timeout(timeout: Timeout) -> Option<Duration> {
    match timeout {
        Timeout::Enabled(duration) => Some(core_duration_to_std(&duration)),
        Timeout::Disabled => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn resolve_per_try_timeout(timeout: Timeout, retry: &RetryPolicy) -> Option<Duration> {
    match retry {
        RetryPolicy::Enabled { per_try, .. } => match per_try {
            TryTimeout::Enabled(duration) => Some(core_duration_to_std(duration)),
            TryTimeout::Inherit => resolve_route_timeout(timeout),
            TryTimeout::Disabled => None,
            _ => None,
        },
        RetryPolicy::Disabled => resolve_route_timeout(timeout),
        _ => resolve_route_timeout(timeout),
    }
}

fn calculate_path_rewrite(
    route: &pavis_core::Route,
    uri_path: &str,
    uri_query: Option<&str>,
) -> Option<Uri> {
    match &route.rewrite.path {
        pavis_core::RewritePath::Disabled => None,
        pavis_core::RewritePath::Prefix { from: _, to } => {
            let new_path = match &route.matcher.path {
                PathMatch::Prefix { path } => {
                    uri_path.strip_prefix(path.0.as_str()).map(|suffix| {
                        let mut path = String::with_capacity(to.0.len() + suffix.len());
                        path.push_str(&to.0);
                        path.push_str(suffix);
                        Cow::Owned(path)
                    })
                }
                PathMatch::Exact { path } => {
                    (uri_path == path.0.as_str()).then_some(Cow::Borrowed(to.0.as_str()))
                }
                PathMatch::Regex { .. } => None,
                #[allow(unreachable_patterns)]
                &_ => None,
            };

            match new_path {
                Some(mut path) => {
                    if let Some(query) = uri_query {
                        let mut owned = match path {
                            Cow::Borrowed(path) => {
                                let mut owned = String::with_capacity(path.len() + 1 + query.len());
                                owned.push_str(path);
                                owned
                            }
                            Cow::Owned(mut owned) => {
                                owned.reserve(1 + query.len());
                                owned
                            }
                        };
                        owned.push('?');
                        owned.push_str(query);
                        path = Cow::Owned(owned);
                    }

                    match Uri::builder().path_and_query(path.as_ref()).build() {
                        Ok(uri) => Some(uri),
                        Err(err) => {
                            tracing::warn!(
                                error = %err,
                                rewrite = %to.0,
                                "Failed to apply path rewrite"
                            );
                            None
                        }
                    }
                }
                None => {
                    if matches!(route.matcher.path, PathMatch::Regex { .. }) {
                        tracing::warn!(
                            route = %route_path(route),
                            "Skipping path rewrite for regex match"
                        );
                    } else {
                        tracing::warn!(
                            route = %route_path(route),
                            path = %uri_path,
                            "Skipping path rewrite due to unmatched prefix"
                        );
                    }
                    None
                }
            }
        }
        #[allow(unreachable_patterns)]
        &_ => None,
    }
}

fn extract_client_identity(_session: &Session) -> Option<String> {
    // TODO: Implement peer certificate extraction for Rustls mode in Pingora 0.6.0.
    // The current Stream trait does not expose peer_certificate in a backend-agnostic way.
    None
}

fn resolve_sni(
    sni: &pavis_core::SniName,
    authority_override: Option<&Hostname>,
    endpoint_host: Option<&Hostname>,
) -> Option<Hostname> {
    match sni {
        pavis_core::SniName::Name(name) => Some(name.clone()),
        pavis_core::SniName::Auto => authority_override
            .cloned()
            .or_else(|| endpoint_host.cloned()),
        pavis_core::SniName::Disabled => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn endpoint_host_for_sni(
    upstream: &pavis_core::Upstream,
    endpoint: &pavis_core::Endpoint,
) -> Option<Hostname> {
    match &endpoint.address {
        EndpointAddr::Dns { host, .. } => Some(host.clone()),
        EndpointAddr::Ip { .. } => {
            if matches!(
                upstream.discovery,
                Discovery::Logical | Discovery::Strict { .. }
            ) {
                let mut selected: Option<&Hostname> = None;
                for endpoint in &upstream.endpoints {
                    if let EndpointAddr::Dns { host, .. } = &endpoint.address {
                        match selected {
                            None => selected = Some(host),
                            Some(existing) => {
                                if existing.0 != host.0 {
                                    return None;
                                }
                            }
                        }
                    }
                }
                selected.cloned()
            } else {
                None
            }
        }
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn resolve_endpoint_addr(endpoint: &pavis_core::Endpoint) -> Result<SocketAddr> {
    match &endpoint.address {
        EndpointAddr::Ip { address, port } => Ok(SocketAddr::new(*address, port.0.get())),
        EndpointAddr::Dns { host, port } => {
            let mut addrs = match (host.0.as_str(), port.0.get()).to_socket_addrs() {
                Ok(addrs) => addrs,
                Err(err) => {
                    return Error::e_explain(
                        InternalError,
                        format!("DNS resolution failed for {}:{} ({})", host.0, port.0, err),
                    );
                }
            };
            match addrs.next() {
                Some(addr) => Ok(addr),
                None => Error::e_explain(
                    InternalError,
                    format!(
                        "DNS resolution returned no addresses for {}:{}",
                        host.0, port.0
                    ),
                ),
            }
        }
        #[allow(unreachable_patterns)]
        _ => Error::e_explain(InternalError, "Unknown endpoint address type"),
    }
}

#[allow(dead_code)]
pub(crate) fn is_authorized(
    principal: &pavis_core::Principal,
    client_identity: Option<&str>,
) -> bool {
    match principal {
        pavis_core::Principal::Any => true,
        pavis_core::Principal::Authenticated { spiffe } => {
            client_identity.is_some_and(|identity| identity == spiffe.as_str())
        }
        pavis_core::Principal::Prefix { prefix } => {
            client_identity.is_some_and(|identity| identity.starts_with(prefix.as_str()))
        }
        #[allow(unreachable_patterns)]
        _ => false,
    }
}

#[async_trait]
impl ProxyHttp for Proxy {
    type CTX = RouterContext;

    fn new_ctx(&self) -> Self::CTX {
        RouterContext {
            upstream_name: None,
            upstream_endpoint: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: std::time::Instant::now(),
            client_identity: None,
            rbac_denied: false,
            route_timeout: Timeout::Disabled,
            retry_policy: RetryPolicy::Disabled,
            retry_attempts: 0,
            upstream_timing: crate::proxy::context::UpstreamTiming::NotStarted,
            route_pattern: crate::proxy::context::RoutePattern::NotMatched,
            req_id: generate_request_id(),
            span: crate::proxy::context::TracingSpan::Disabled,
            pool_permit: None,
            circuit_breaker_permit: None,
            runtime_state: None,
            retry_ctx: None,
            buffered_body: None,
            rewritten_uri: None,
            rewritten_host: None,
        }
    }

    async fn early_request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<()> {
        if ctx.client_identity.is_none() {
            ctx.client_identity = extract_client_identity(session);
        }
        Ok(())
    }

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let upstream_name = match &ctx.upstream_name {
            Some(name) => name,
            None => return Error::e_explain(InternalError, "No upstream selected"),
        };

        // O(1) lookup using Manager; runtime state must be pinned by request_filter.
        let state = match ctx.runtime_state.clone() {
            Some(state) => state,
            None => {
                let route = ctx.route_pattern.as_label();
                return Error::e_explain(
                    InternalError,
                    format!(
                        "missing runtime snapshot: request_id={} route={} upstream={}",
                        ctx.req_id, route, upstream_name.0
                    ),
                );
            }
        };
        let cluster = match state.upstream_manager.get(upstream_name.0.as_str()) {
            Some(u) => u,
            None => return Error::e_explain(InternalError, "Upstream not found in config"),
        };

        // Handle retries and backoff
        if let Some(retry_ctx) = &ctx.retry_ctx
            && retry_ctx.attempt > 1
        {
            // IMPORTANT: Release permits from previous attempt before waiting for new ones.
            // This prevents deadlocks where retrying requests hold all slots while waiting for backoff.
            ctx.pool_permit = None;
            ctx.circuit_breaker_permit = None;

            retry_ctx.apply_backoff().await;

            // Check global deadline
            if retry_ctx.is_deadline_exceeded() {
                return Error::e_explain(
                    ErrorType::HTTPStatus(504),
                    "ERR_REQUEST_TIMEOUT_GLOBAL: total request time exceeded deadline",
                );
            }

            // Check body replayability
            if let Some(body) = &ctx.buffered_body
                && let pavis_core::RetryPolicy::Enabled {
                    fail_on_non_replayable_retry,
                    ..
                } = &ctx.retry_policy
                && body
                    .handle_non_replayable(*fail_on_non_replayable_retry)
                    .is_err()
            {
                return Error::e_explain(
                    ErrorType::HTTPStatus(500),
                    "ERR_RETRY_BODY_NOT_REPLAYABLE: body size exceeds buffer limit",
                );
            }
        }

        let pool_permit = match cluster.acquire_pool_permit().await {
            Ok(permit) => permit,
            Err(PoolRejection::QueueFull) => {
                tracing::info!(upstream = %upstream_name.0, "Upstream pool queue full");
                let mut err = Error::explain(
                    ErrorType::HTTPStatus(503),
                    "ERR_UPSTREAM_POOL_FULL: connection pool is full",
                );

                if let Some(retry_ctx) = &mut ctx.retry_ctx
                    && retry_ctx.is_retryable(pavis_core::RetryReason::PoolFull)
                    && retry_ctx.can_retry()
                {
                    retry_ctx.next_attempt(pavis_core::RetryReason::PoolFull);
                    err.set_retry(true);
                }
                return Err(err);
            }
            Err(PoolRejection::QueueTimeout) => {
                tracing::info!(
                    upstream = %upstream_name.0,
                    "Upstream pool queue wait timed out"
                );
                let mut err = Error::explain(
                    ErrorType::HTTPStatus(503),
                    "ERR_UPSTREAM_POOL_FULL: connection pool wait timed out",
                );

                if let Some(retry_ctx) = &mut ctx.retry_ctx
                    && retry_ctx.is_retryable(pavis_core::RetryReason::PoolFull)
                    && retry_ctx.can_retry()
                {
                    retry_ctx.next_attempt(pavis_core::RetryReason::PoolFull);
                    err.set_retry(true);
                }
                return Err(err);
            }
            Err(PoolRejection::Closed) => {
                tracing::error!(upstream = %upstream_name.0, "Upstream pool closed");
                return Error::e_explain(InternalError, "Upstream pool unavailable");
            }
        };

        let permit = match cluster.acquire_breaker_permit().await {
            Ok(permit) => permit,
            Err(CircuitBreakerRejection::PendingLimit | CircuitBreakerRejection::Closed) => {
                tracing::info!(
                    upstream = %upstream_name.0,
                    "Circuit breaker rejected request"
                );
                return Error::e_explain(
                    ErrorType::HTTPStatus(503),
                    "circuit breaker rejected request",
                );
            }
        };
        ctx.circuit_breaker_permit = permit;

        let endpoint = match cluster.select_endpoint() {
            Some(e) => e,
            None => {
                ctx.circuit_breaker_permit = None;
                return Error::e_explain(InternalError, "Upstream has no endpoints");
            }
        };
        ctx.upstream_endpoint = Some(endpoint.address.clone());

        let upstream = &cluster.config;

        let addr = resolve_endpoint_addr(&endpoint)?;
        let endpoint_host = endpoint_host_for_sni(upstream, &endpoint);

        tracing::debug!(
            upstream = %upstream_name.0,
            endpoint = %addr,
            lb = ?upstream.balancer,
            http = ?upstream.protocol,
            "forwarding request"
        );

        let (use_tls, sni, verify_mode, cert, ca) = match &upstream.tls {
            pavis_core::TlsPolicy::Disabled => (false, None, None, None, None),
            pavis_core::TlsPolicy::Enabled {
                verify,
                sni,
                cert,
                ca,
            } => {
                let sni_value = match sni {
                    pavis_core::SniName::Name(name) => {
                        tracing::info!(
                            upstream = %upstream_name.0,
                            sni = %name.0,
                            "Using explicit SNI for upstream"
                        );
                        Some(name.clone())
                    }
                    _ => resolve_sni(sni, ctx.sni_override.as_ref(), endpoint_host.as_ref()),
                };
                (true, sni_value, Some(*verify), Some(cert), Some(ca))
            }
            #[allow(unreachable_patterns)]
            _ => (false, None, None, None, None),
        };

        if use_tls && matches!(verify_mode, Some(pavis_core::TlsVerify::Full)) && sni.is_none() {
            return Error::e_explain(
                InternalError,
                "TLS verify=full requires SNI (auto or explicit)",
            );
        }
        let sni_string = sni.map(|name| name.0).unwrap_or_else(String::new);
        let mut peer = HttpPeer::new(addr, use_tls, sni_string);

        if let Some(mode) = verify_mode {
            match mode {
                pavis_core::TlsVerify::Disabled => {
                    peer.options.verify_hostname = false;
                    peer.options.verify_cert = false;
                }
                pavis_core::TlsVerify::CaOnly => {
                    peer.options.verify_hostname = false;
                    peer.options.verify_cert = true;
                }
                pavis_core::TlsVerify::Full => {
                    peer.options.verify_hostname = true;
                    peer.options.verify_cert = true;
                }
                #[allow(unreachable_patterns)]
                _ => {
                    // Default to disabled for unknown verify modes
                    peer.options.verify_hostname = false;
                    peer.options.verify_cert = false;
                }
            }
        }

        // Configure client certificate for outbound mTLS
        if let Some(cert_config) = cert {
            match cert_config {
                pavis_core::ClientCert::Disabled => {
                    // No client certificate
                }
                pavis_core::ClientCert::Enabled {
                    cert_path,
                    key_path,
                    ..
                } => {
                    tracing::debug!(
                        cert_path = %cert_path.0,
                        key_path = %key_path.0,
                        "Configuring client certificate for upstream connection"
                    );
                    let client_cert_key = cluster.client_cert_key().ok_or_else(|| {
                        Error::explain(InternalError, "Client certificate not loaded")
                    })?;
                    peer.client_cert_key = Some(client_cert_key);
                }
                #[allow(unreachable_patterns)]
                _ => {
                    // Unknown client cert configuration
                }
            }
        }

        // Configure CA bundle for upstream TLS verification
        // TODO: Pingora's rustls connector does not currently support per-peer CA certificates.
        // See: https://github.com/cloudflare/pingora/blob/main/pingora-core/src/connectors/tls/rustls/mod.rs
        // The rustls connector has a TODO comment: "setup CA/verify cert store from peer"
        // Currently, the CA bundle is set here but will be ignored by the connector.
        // Options to fix:
        // 1. Wait for pingora to implement this feature
        // 2. Switch to OpenSSL backend (features = ["proxy", "openssl"])
        // 3. Implement a custom rustls connector that respects peer.get_ca()
        if let Some(ca_config) = ca {
            match ca_config {
                pavis_core::UpstreamCa::System => {
                    // Use system CA bundle (default)
                    tracing::debug!("Using system CA bundle for upstream TLS verification");
                }
                pavis_core::UpstreamCa::File { path } => {
                    // Load CA bundle from cluster
                    if let Some(ca_bundle) = cluster.ca_bundle() {
                        tracing::debug!(
                            upstream = %upstream_name.0,
                            ca_path = %path.0,
                            ca_count = ca_bundle.len(),
                            "Setting custom CA bundle for upstream TLS verification (NOTE: Currently not used by pingora rustls connector)"
                        );
                        peer.options.ca = Some(ca_bundle);
                    } else {
                        tracing::warn!(
                            upstream = %upstream_name.0,
                            ca_path = %path.0,
                            "CA bundle configured but not loaded in cluster"
                        );
                    }
                }
                #[allow(unreachable_patterns)]
                _ => {
                    // Unknown CA configuration
                }
            }
        }

        // Configure HTTP version
        match upstream.protocol {
            HttpVersion::H1 => peer.options.set_http_version(1, 1),
            HttpVersion::H2 => peer.options.set_http_version(2, 2),
            HttpVersion::H2H1 => peer.options.set_http_version(2, 1),
            #[allow(unreachable_patterns)]
            _ => peer.options.set_http_version(1, 1), // Default to H1
        }

        // Configure connection pooling
        peer.options.idle_timeout = match upstream.pool.idle {
            pavis_core::IdleTimeout::Disabled => None,
            pavis_core::IdleTimeout::Enabled(duration) => {
                Some(Duration::from_millis(duration.0.get() as u64))
            }
            #[allow(unreachable_patterns)]
            _ => None,
        };
        peer.options.connection_timeout = match upstream.pool.connect {
            ConnectTimeout::Disabled => None,
            ConnectTimeout::Enabled(duration) => {
                Some(Duration::from_millis(duration.0.get() as u64))
            }
            #[allow(unreachable_patterns)]
            _ => None,
        };
        let per_try_timeout = resolve_per_try_timeout(ctx.route_timeout, &ctx.retry_policy);
        peer.options.read_timeout = per_try_timeout;
        peer.options.write_timeout = per_try_timeout;

        // Track upstream timing
        ctx.start_upstream();
        ctx.pool_permit = pool_permit;

        Ok(Box::new(peer))
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        if let Some(metrics) = &self.telemetry.metrics {
            metrics.increment_active_connections();
        }

        let req_header = session.req_header();
        let host_header = req_header.headers.get("Host").and_then(|h| h.to_str().ok());
        let uri_path = req_header.uri.path();

        // Check if tracing is initialized AND enabled in current config
        let state = self.state.load();
        ctx.runtime_state = Some(state.clone());

        let tracing_enabled = if let pavis_core::TracingPolicy::Enabled { sampling, .. } =
            &state.config.telemetry.tracing
        {
            // Simple sampling check (0 or >0) for enabling the span creation.
            // Detailed sampling happens in the OTel SDK, but if sampling is 0, we can skip span creation.
            // However, we need the span for context propagation even if not sampled?
            // If sampling is 0, the sampler will drop it.
            // But we check self.telemetry.tracing to see if the RUNTIME is available.
            self.telemetry.tracing.get().is_some() && sampling.0 > 0
        } else {
            false
        };

        if tracing_enabled {
            let span = tracing::info_span!(
                "http_request",
                http.method = %req_header.method,
                http.target = %uri_path,
                http.host = ?host_header,
                http.request_id = %ctx.req_id,
                otel.kind = "server",
            );

            ctx.span = TracingSpan::Active(span);
        }

        tracing::debug!(
            method = %req_header.method,
            path = %uri_path,
            host = ?host_header,
            "incoming request"
        );

        let verdict = state.router.match_request(
            host_header,
            uri_path,
            req_header.method.as_str(),
            req_header,
        );

        if let Some(metrics) = &self.telemetry.metrics {
            metrics.record_route_match(&verdict);
        }

        if let Some((vhost, route)) = verdict.selection {
            tracing::trace!(host = %vhost.host.0, path = %route_path(route), "matched route");

            ctx.route_pattern = crate::proxy::context::RoutePattern::Matched {
                pattern: Arc::from(route_path(route)),
            };

            if let crate::proxy::context::RoutePattern::Matched { ref pattern } = ctx.route_pattern
                && let TracingSpan::Active(ref span) = ctx.span
            {
                span.record("route.pattern", pattern.as_ref());
            }

            apply_route_headers(ctx, route);

            // RBAC Authorization
            if !is_authorized(&route.principal, ctx.client_identity.as_deref()) {
                tracing::info!(
                    request_id = %ctx.req_id,
                    principal = ?route.principal,
                    client_identity = ?ctx.client_identity,
                    "RBAC access denied"
                );
                ctx.rbac_denied = true;
                let _ = session.respond_error(403).await;
                return Ok(true);
            }

            // Handle path rewrite
            if let Some(new_uri) = calculate_path_rewrite(route, uri_path, req_header.uri.query()) {
                tracing::debug!(
                    original = %uri_path,
                    rewritten = %new_uri.path(),
                    "Calculated path rewrite"
                );
                ctx.rewritten_uri = Some(new_uri);
            }

            // Handle host rewrite
            if let pavis_core::RewriteHost::Literal { host } = &route.rewrite.host {
                tracing::debug!(
                    original = ?host_header,
                    rewritten = %host.0,
                    "Calculated host rewrite"
                );
                ctx.rewritten_host = Some(host.clone());
            }

            // Initialize RetryContext if enabled
            if let pavis_core::RetryPolicy::Enabled {
                max_request_body_buffer_bytes,
                ..
            } = &ctx.retry_policy
            {
                let upstream_name = match &route.action {
                    RouteAction::Forward(destinations) if !destinations.is_empty() => {
                        // We use the first upstream for the context; load balancing happens per-try
                        destinations[0].upstream.0.clone()
                    }
                    _ => "unknown".to_string(),
                };

                let request_timeout_ms = resolve_route_timeout(ctx.route_timeout)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(60000); // Default to 60s if no timeout configured

                ctx.retry_ctx = Some(RetryContext::new(
                    ctx.retry_policy.clone(),
                    request_timeout_ms,
                    self.telemetry.metrics.clone(),
                    upstream_name.clone(),
                ));

                // Buffer request body for replay if it exists
                // Note: Pingora 0.6.0 session doesn't have is_body_empty,
                // we'll try to read and see.
                let mut body_bytes = Vec::new();
                let limit = *max_request_body_buffer_bytes;

                while let Some(chunk) = session.read_request_body().await? {
                    if (body_bytes.len() as u64) + (chunk.len() as u64) > limit {
                        // Body exceeds buffer limit, stop buffering and mark as streaming
                        tracing::debug!(
                            limit = limit,
                            upstream = %upstream_name,
                            "Request body exceeds buffer limit, marking as non-replayable"
                        );
                        body_bytes.extend_from_slice(&chunk);
                        break;
                    }
                    body_bytes.extend_from_slice(&chunk);
                }

                ctx.buffered_body = Some(crate::retry::BufferedBody::new(
                    body_bytes,
                    limit,
                    self.telemetry.metrics.clone(),
                    &upstream_name,
                ));
            }

            match &route.action {
                RouteAction::Forward(destinations) => {
                    let total_weight: u32 =
                        destinations.iter().map(|d| d.weight.0.get() as u32).sum();
                    if total_weight == 0 {
                        return Ok(false);
                    }

                    let mut rng = rand::rng();
                    let mut pick = rng.random_range(0..total_weight);

                    for dest in destinations {
                        let weight = dest.weight.0.get() as u32;
                        if pick < weight {
                            ctx.upstream_name = Some(dest.upstream.clone());
                            tracing::debug!(
                                upstream = %dest.upstream.0,
                                "Selected upstream"
                            );

                            if let TracingSpan::Active(ref span) = ctx.span {
                                span.record("upstream", dest.upstream.0.as_str());
                            }

                            break;
                        }
                        pick -= weight;
                    }
                    return Ok(false);
                }
                RouteAction::Redirect { status, location } => {
                    let status_code = *status;
                    let location_url = location.clone();
                    let response_headers = route.response_headers.clone();
                    drop(state);

                    let mut resp = ResponseHeader::build(status_code, None)?;
                    resp.insert_header("Location", location_url.as_str())?;
                    resp.insert_header("Content-Length", "0")?;
                    apply_response_headers(&mut resp, &response_headers)?;
                    session.write_response_header(Box::new(resp), true).await?;
                    return Ok(true);
                }
                RouteAction::Direct { status, body } => {
                    let status_code = *status;
                    let body_content = body.clone();
                    let response_headers = route.response_headers.clone();
                    drop(state);

                    let mut resp = ResponseHeader::build(status_code, None)?;
                    resp.insert_header("Content-Type", "text/plain")?;
                    resp.insert_header("Content-Length", body_content.len().to_string())?;
                    apply_response_headers(&mut resp, &response_headers)?;
                    session.write_response_header(Box::new(resp), false).await?;
                    session
                        .write_response_body(Some(body_content.into_bytes().into()), true)
                        .await?;
                    return Ok(true);
                }
                #[allow(unreachable_patterns)]
                _ => {}
            }
        }

        let _ = session.respond_error(404).await;
        Ok(true)
    }

    fn fail_to_connect(
        &self,
        _session: &mut Session,
        _peer: &HttpPeer,
        ctx: &mut Self::CTX,
        mut e: Box<Error>,
    ) -> Box<Error> {
        let mut retried = false;

        if let Some(retry_ctx) = &mut ctx.retry_ctx {
            let req_header = _session.req_header();
            let method = pavis_core::HttpMethod::from(req_header.method.as_str());

            if retry_ctx.is_method_allowed(&method) {
                let reason = if matches!(e.etype(), ErrorType::ConnectTimedout) {
                    pavis_core::RetryReason::ConnectTimeout
                } else {
                    pavis_core::RetryReason::ConnectError
                };

                if retry_ctx.is_retryable(reason) && retry_ctx.can_retry() {
                    let can_replay = ctx
                        .buffered_body
                        .as_ref()
                        .map(|b| b.can_replay())
                        .unwrap_or(true);

                    if can_replay {
                        retry_ctx.next_attempt(reason);
                        retried = true;
                    } else if let pavis_core::RetryPolicy::Enabled {
                        fail_on_non_replayable_retry: true,
                        ..
                    } = &ctx.retry_policy
                    {
                        let mut err = Error::new(ErrorType::HTTPStatus(500));
                        err.set_context(
                            "ERR_RETRY_BODY_NOT_REPLAYABLE: body size exceeds buffer limit",
                        );
                        e = err;
                        e.set_retry(false);
                    } else {
                        tracing::warn!(
                            "Request body not replayable, retry aborted (returning last response)"
                        );
                    }
                }
            }
        }

        if retried {
            e.set_retry(true);
        }

        e
    }

    fn error_while_proxy(
        &self,
        peer: &HttpPeer,
        session: &mut Session,
        e: Box<Error>,
        ctx: &mut Self::CTX,
        _client_reused: bool,
    ) -> Box<Error> {
        let mut e = e.more_context(format!("Peer: {}", peer));
        e.set_retry(false);

        if _client_reused {
            e.set_retry(true);
            return e;
        }

        if let Some(retry_ctx) = &mut ctx.retry_ctx {
            let req_header = session.req_header();
            let method = pavis_core::HttpMethod::from(req_header.method.as_str());

            if retry_ctx.is_method_allowed(&method) {
                let reason = if matches!(e.etype(), ErrorType::ReadTimedout) {
                    pavis_core::RetryReason::ReadTimeout
                } else {
                    // For other errors while proxying, we might not want to retry
                    // unless it is a reset and the policy allows it.
                    // Current simple mapping:
                    pavis_core::RetryReason::StatusCode // Placeholder
                };

                if retry_ctx.is_retryable(reason) && retry_ctx.can_retry() {
                    e.set_retry(true);
                    retry_ctx.next_attempt(reason);
                }
            }
        }

        // Preserve reuse safety by avoiding retries when the buffer is truncated.
        if session.as_ref().retry_buffer_truncated() {
            e.set_retry(false);
        }

        e
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        if let Some(uri) = &ctx.rewritten_uri {
            upstream_request.set_uri(uri.clone());
        }
        if let Some(host) = &ctx.rewritten_host {
            upstream_request.insert_header("Host", host.0.as_str())?;
        }
        if let TracingSpan::Active(ref span) = ctx.span {
            let context = span.context();
            opentelemetry::global::get_text_map_propagator(|propagator| {
                propagator.inject_context(&context, &mut HeaderInjector(upstream_request))
            });
        }
        apply_request_headers(upstream_request, &ctx.request_headers)
    }

    fn upstream_response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        apply_response_headers(upstream_response, &ctx.response_headers)
    }

    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        if let Some(retry_ctx) = &mut ctx.retry_ctx {
            let req_header = _session.req_header();
            let method = pavis_core::HttpMethod::from(req_header.method.as_str());

            if retry_ctx.is_method_allowed(&method) {
                let status = upstream_response.status.as_u16();
                if retry_ctx.is_status_code_retryable(status) && retry_ctx.can_retry() {
                    let can_replay = ctx
                        .buffered_body
                        .as_ref()
                        .map(|b| b.can_replay())
                        .unwrap_or(true);

                    if can_replay {
                        let mut err = Error::new(ErrorType::HTTPStatus(status));
                        err.as_up();
                        err.set_retry(true);
                        err.set_context("retryable upstream response");
                        retry_ctx.next_attempt(pavis_core::RetryReason::StatusCode);
                        return Err(err);
                    } else if let pavis_core::RetryPolicy::Enabled {
                        fail_on_non_replayable_retry: true,
                        ..
                    } = &ctx.retry_policy
                    {
                        return Error::e_explain(
                            ErrorType::HTTPStatus(500),
                            "ERR_RETRY_BODY_NOT_REPLAYABLE: body size exceeds buffer limit",
                        );
                    } else {
                        tracing::warn!(
                            "Request body not replayable, retry aborted (returning last response)"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    async fn logging(&self, session: &mut Session, _e: Option<&Error>, ctx: &mut Self::CTX) {
        self.telemetry.access_log.log(session, ctx).await;

        if let Some(retry_ctx) = &ctx.retry_ctx {
            let status = session
                .response_written()
                .map(|r| r.status.as_u16())
                .unwrap_or(0);
            let success = status > 0 && status < 500;
            retry_ctx.record_outcome(success);
        }

        if let Some(metrics) = &self.telemetry.metrics {
            let req = session.req_header();
            let method = req.method.as_str();
            let status = session
                .response_written()
                .map(|r| r.status.as_u16())
                .unwrap_or(0);

            match &ctx.route_pattern {
                crate::proxy::context::RoutePattern::Matched { pattern } => {
                    let route_pattern = pattern.as_ref();

                    let upstream = ctx
                        .upstream_name
                        .as_ref()
                        .map(|u| u.0.as_str())
                        .unwrap_or("-");

                    let duration_secs = ctx.start_time.elapsed().as_secs_f64();

                    metrics.record_request(method, route_pattern, status, upstream, duration_secs);

                    if let Some(upstream_name) = &ctx.upstream_name {
                        let upstream_duration_secs = match &ctx.upstream_timing {
                            crate::proxy::context::UpstreamTiming::Started(start) => {
                                start.elapsed().as_secs_f64()
                            }
                            crate::proxy::context::UpstreamTiming::NotStarted => duration_secs,
                        };

                        metrics.record_upstream_request(
                            &upstream_name.0,
                            status,
                            upstream_duration_secs,
                        );
                    }
                }
                crate::proxy::context::RoutePattern::NotMatched => {
                    metrics.record_metrics_label_dropped();
                }
            }

            metrics.decrement_active_connections();
        }

        if let (Some(state), Some(upstream_name), Some(endpoint)) = (
            ctx.runtime_state.as_ref(),
            ctx.upstream_name.as_ref(),
            ctx.upstream_endpoint.as_ref(),
        ) && let Some(cluster) = state.upstream_manager.get(upstream_name.0.as_str())
        {
            let status = session
                .response_written()
                .map(|r| r.status.as_u16())
                .unwrap_or(0);
            let outcome = if _e.is_some() || status >= 500 {
                UpstreamOutcome::Failure
            } else if status > 0 {
                UpstreamOutcome::Success
            } else {
                UpstreamOutcome::Failure
            };
            cluster.record_outcome(endpoint, outcome);
        }

        ctx.pool_permit.take();
        ctx.circuit_breaker_permit.take();

        if let TracingSpan::Active(ref span) = ctx.span
            && let Some(response) = session.response_written()
        {
            let status_code = response.status.as_u16();
            span.record("http.status_code", status_code);

            if status_code >= 500 {
                span.record("error", true);
                span.record("error.type", "server_error");
            } else if status_code >= 400 {
                span.record("error", true);
                span.record("error.type", "client_error");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_timestamp_handles_underflow() {
        let before_epoch = std::time::UNIX_EPOCH - std::time::Duration::from_secs(1);
        let timestamp = request_id_timestamp(before_epoch);
        assert_eq!(timestamp, 0);

        let id = RequestId::from_parts(timestamp, 1);
        assert!(id.as_str().starts_with("req-0-"));
    }
}

fn route_path(route: &pavis_core::Route) -> &str {
    match &route.matcher.path {
        PathMatch::Prefix { path } => path.0.as_str(),
        PathMatch::Exact { path } => path.0.as_str(),
        PathMatch::Regex { path } => path.0.as_str(),
        #[allow(unreachable_patterns)]
        &_ => "",
    }
}

#[cfg(test)]
mod service_tests;

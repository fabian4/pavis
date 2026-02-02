use crate::proxy::context::{RequestTelemetry, RouterContext, TracingSpan};
use crate::proxy::header_ops::{apply_request_headers, apply_response_headers};
use crate::retry::RetryContext;
use crate::upstream::cluster::{CircuitBreakerRejection, PoolRejection, UpstreamOutcome};
use async_trait::async_trait;
use pavis_core::{
    HeadersPolicy, Hostname, HttpVersion, RetryPolicy, RouteAction, Timeout, UpstreamName,
};
use pingora::ErrorType;
use pingora::http::RequestHeader;
use pingora::http::ResponseHeader;
use pingora::prelude::*;
use pingora::protocols::Digest;
use pingora::proxy::{ProxyHttp, Session};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::request_planning::{
    HeaderInjector, apply_route_headers, calculate_path_rewrite, endpoint_host_for_sni,
    extract_client_identity, generate_request_id, is_authorized, resolve_endpoint_addr,
    resolve_per_try_timeout, resolve_route_timeout, resolve_sni, reuse_key_hash, route_path,
};
use super::state::Proxy;

static POOL_KEY_LOG_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SNI_FRAGMENT_WARN_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[async_trait]
impl ProxyHttp for Proxy {
    type CTX = RouterContext;

    fn new_ctx(&self) -> Self::CTX {
        RouterContext {
            telemetry: RequestTelemetry::new(generate_request_id()),
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

        let state = match ctx.runtime_state.clone() {
            Some(state) => state,
            None => {
                let route = ctx.route_pattern.as_label();
                return Error::e_explain(
                    InternalError,
                    format!(
                        "missing runtime snapshot: request_id={} route={} upstream={}",
                        ctx.request_id(),
                        route,
                        upstream_name.0
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
            ctx.pool_permit = None;
            ctx.circuit_breaker_permit = None;

            retry_ctx.apply_backoff().await;

            if retry_ctx.is_deadline_exceeded() {
                return Error::e_explain(
                    ErrorType::HTTPStatus(504),
                    "ERR_REQUEST_TIMEOUT_GLOBAL: total request time exceeded deadline",
                );
            }

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

        let builder = UpstreamPeerBuilder::new(&self.telemetry);
        let peer = builder.build(
            ctx,
            upstream_name,
            upstream,
            cluster.as_ref(),
            &endpoint,
            endpoint_host,
            addr,
        )?;

        ctx.start_upstream();
        ctx.pool_permit = pool_permit;

        Ok(Box::new(peer))
    }

    async fn connected_to_upstream(
        &self,
        _session: &mut Session,
        reused: bool,
        _peer: &HttpPeer,
        #[cfg(unix)] _fd: std::os::unix::io::RawFd,
        #[cfg(windows)] _sock: std::os::windows::io::RawSocket,
        _digest: Option<&Digest>,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        if let (Some(metrics), Some(upstream)) = (
            &self.telemetry.metrics,
            ctx.upstream_name.as_ref().map(|name| name.0.as_str()),
        ) {
            if reused {
                metrics.record_connection_reused(upstream);
            } else {
                metrics.record_connection_new(upstream, "new_connection");
            }
        }
        Ok(())
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        if let Some(metrics) = &self.telemetry.metrics {
            metrics.increment_active_connections();
        }

        let req_header = session.req_header();
        let host_header = req_header.headers.get("Host").and_then(|h| h.to_str().ok());
        let uri_path = req_header.uri.path();

        let state = self.state.load();
        let request_id = ctx.request_id();

        let tracing_enabled = if let pavis_core::TracingPolicy::Enabled { sampling, .. } =
            &state.config.telemetry.tracing
        {
            self.telemetry.tracing.get().is_some() && sampling.0 > 0
        } else {
            false
        };

        {
            let mut phase = ctx.routing_phase();
            phase.attach_runtime(state.clone());
            if tracing_enabled {
                let span = tracing::info_span!(
                    "http_request",
                    http.method = %req_header.method,
                    http.target = %uri_path,
                    http.host = ?host_header,
                    http.request_id = %request_id,
                    otel.kind = "server",
                );
                phase.enable_tracing(span);
            }
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

            {
                let mut route_phase = ctx
                    .routing_phase()
                    .record_route(Arc::from(route_path(route)));

                route_phase.record_route_span();

                apply_route_headers(route_phase.ctx_mut(), route);

                if !is_authorized(&route.principal, route_phase.client_identity()) {
                    tracing::info!(
                        request_id = %route_phase.request_id(),
                        principal = ?route.principal,
                        client_identity = ?route_phase.client_identity(),
                        "RBAC access denied"
                    );
                    route_phase.mark_rbac_denied();
                    let _ = session.respond_error(403).await;
                    return Ok(true);
                }

                if let Some(new_uri) =
                    calculate_path_rewrite(route, uri_path, req_header.uri.query())
                {
                    tracing::debug!(
                        original = %uri_path,
                        rewritten = %new_uri.path(),
                        "Calculated path rewrite"
                    );
                    route_phase.set_rewritten_uri(new_uri);
                }

                if let pavis_core::RewriteHost::Literal { host } = &route.rewrite.host {
                    tracing::debug!(
                        original = ?host_header,
                        rewritten = %host.0,
                        "Calculated host rewrite"
                    );
                    route_phase.set_rewritten_host(host.clone());
                }

                let retry_policy = route_phase.retry_policy().clone();
                if let pavis_core::RetryPolicy::Enabled {
                    max_request_body_buffer_bytes,
                    ..
                } = &retry_policy
                {
                    let route_timeout = route_phase.route_timeout();
                    let upstream_name = match &route.action {
                        RouteAction::Forward(destinations) if !destinations.is_empty() => {
                            destinations[0].upstream.0.clone()
                        }
                        _ => "unknown".to_string(),
                    };

                    let request_timeout_ms = resolve_route_timeout(route_timeout)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(60000);

                    route_phase.set_retry_context(RetryContext::new(
                        retry_policy.clone(),
                        request_timeout_ms,
                        self.telemetry.metrics.clone(),
                        upstream_name.clone(),
                    ));

                    let mut body_bytes = Vec::new();
                    let limit = *max_request_body_buffer_bytes;

                    while let Some(chunk) = session.read_request_body().await? {
                        if (body_bytes.len() as u64) + (chunk.len() as u64) > limit {
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

                    let buffered = crate::retry::BufferedBody::new(
                        body_bytes,
                        limit,
                        self.telemetry.metrics.clone(),
                        &upstream_name,
                    );
                    route_phase.set_buffered_body(buffered);
                }

                if let RouteAction::Forward(destinations) = &route.action {
                    let total_weight: u32 =
                        destinations.iter().map(|d| d.weight.0.get() as u32).sum();
                    if total_weight == 0 {
                        return Ok(false);
                    }

                    let mut attempt = route_phase.into_upstream_attempt();
                    let mut rng = rand::rng();
                    let mut pick = rand::Rng::random_range(&mut rng, 0..total_weight);

                    for dest in destinations {
                        let weight = dest.weight.0.get() as u32;
                        if pick < weight {
                            attempt.set_upstream(dest.upstream.clone());
                            tracing::debug!(upstream = %dest.upstream.0, "Selected upstream");
                            attempt.record_upstream_span(dest.upstream.0.as_str());
                            break;
                        }
                        pick -= weight;
                    }

                    return Ok(false);
                }
            }

            match &route.action {
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
                    pavis_core::RetryReason::StatusCode
                };

                if retry_ctx.is_retryable(reason) && retry_ctx.can_retry() {
                    e.set_retry(true);
                    retry_ctx.next_attempt(reason);
                }
            }
        }

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
        if let TracingSpan::Active(span) = ctx.span() {
            let context = span.context();
            opentelemetry::global::get_text_map_propagator(|propagator| {
                propagator.inject_context(&context, &mut HeaderInjector(upstream_request))
            });
        }
        apply_request_headers(upstream_request, &ctx.request_headers)
    }

    async fn upstream_response_filter(
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

        if let TracingSpan::Active(span) = ctx.span()
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

struct UpstreamPeerBuilder<'a> {
    telemetry: &'a Arc<crate::telemetry::Telemetry>,
}

impl<'a> UpstreamPeerBuilder<'a> {
    fn new(telemetry: &'a Arc<crate::telemetry::Telemetry>) -> Self {
        Self { telemetry }
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        &self,
        ctx: &RouterContext,
        upstream_name: &UpstreamName,
        upstream: &pavis_core::Upstream,
        cluster: &crate::upstream::cluster::Cluster,
        _endpoint: &pavis_core::Endpoint,
        endpoint_host: Option<Hostname>,
        addr: std::net::SocketAddr,
    ) -> Result<HttpPeer> {
        let (use_tls, sni_value, verify_mode, cert, ca) = match &upstream.tls {
            pavis_core::TlsPolicy::Disabled => (false, None, None, None, None),
            pavis_core::TlsPolicy::Enabled {
                verify,
                sni,
                cert,
                ca,
                canonical_sni,
                reuse_across_sni,
            } => {
                let canonical = match canonical_sni {
                    pavis_core::CanonicalSni::Disabled => None,
                    pavis_core::CanonicalSni::Enabled { name } => Some(name.clone()),
                    #[allow(unreachable_patterns)]
                    _ => None,
                };

                let sni_value = if let Some(name) = canonical {
                    tracing::info!(
                        upstream = %upstream_name.0,
                        sni = %name.0,
                        "Using canonical SNI for upstream"
                    );
                    Some(name)
                } else if matches!(reuse_across_sni, pavis_core::ReuseAcrossSni::Enabled) {
                    let fixed = match sni {
                        pavis_core::SniName::Name(name) => Some(name.clone()),
                        pavis_core::SniName::Auto => endpoint_host.clone(),
                        pavis_core::SniName::Disabled => None,
                        #[allow(unreachable_patterns)]
                        _ => None,
                    };
                    if SNI_FRAGMENT_WARN_COUNTER
                        .fetch_add(1, Ordering::Relaxed)
                        .is_multiple_of(2048)
                    {
                        tracing::warn!(
                            upstream = %upstream_name.0,
                            sni_mode = ?sni,
                            "reuse_across_sni enabled; upstream will reuse connections across SNI values"
                        );
                    }
                    fixed
                } else {
                    match sni {
                        pavis_core::SniName::Name(name) => {
                            tracing::info!(
                                upstream = %upstream_name.0,
                                sni = %name.0,
                                "Using explicit SNI for upstream"
                            );
                            Some(name.clone())
                        }
                        pavis_core::SniName::Auto => {
                            if ctx.sni_override.is_some()
                                && SNI_FRAGMENT_WARN_COUNTER
                                    .fetch_add(1, Ordering::Relaxed)
                                    .is_multiple_of(2048)
                            {
                                tracing::warn!(
                                    upstream = %upstream_name.0,
                                    "SNI override active without canonical SNI; connection reuse may fragment"
                                );
                            }
                            resolve_sni(sni, ctx.sni_override.as_ref(), endpoint_host.as_ref())
                        }
                        _ => resolve_sni(sni, ctx.sni_override.as_ref(), endpoint_host.as_ref()),
                    }
                };

                (true, sni_value, Some(*verify), Some(cert), Some(ca))
            }
            #[allow(unreachable_patterns)]
            _ => (false, None, None, None, None),
        };

        if use_tls
            && matches!(verify_mode, Some(pavis_core::TlsVerify::Full))
            && sni_value.is_none()
        {
            return Error::e_explain(
                InternalError,
                "TLS verify=full requires SNI (auto or explicit)",
            );
        }

        let sni_label = sni_value.as_ref().map(|name| name.0.as_str()).unwrap_or("");
        if let Some(tracker) = self.telemetry.pool_key_tracker.as_ref() {
            let snapshot = tracker.record(
                upstream_name.0.as_str(),
                reuse_key_hash(&addr, sni_label, verify_mode, cert),
            );
            if let Some(metrics) = &self.telemetry.metrics {
                metrics.record_pool_key_cardinality(
                    upstream_name.0.as_str(),
                    snapshot.cardinality,
                    snapshot.saturated,
                );
            }
            if POOL_KEY_LOG_COUNTER
                .fetch_add(1, Ordering::Relaxed)
                .is_multiple_of(4096)
            {
                tracing::debug!(
                    upstream = %upstream_name.0,
                    endpoint = %addr,
                    sni = %sni_label,
                    verify = ?verify_mode,
                    "Observed upstream pool reuse key"
                );
            }
        }

        let sni_string = sni_value.map(|name| name.0).unwrap_or_else(String::new);
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
                    peer.options.verify_hostname = false;
                    peer.options.verify_cert = false;
                }
            }
        }

        if let Some(cert_config) = cert {
            match cert_config {
                pavis_core::ClientCert::Disabled => {}
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
                _ => {}
            }
        }

        if let Some(ca_config) = ca {
            match ca_config {
                pavis_core::UpstreamCa::System => {
                    tracing::debug!("Using system CA bundle for upstream TLS verification");
                }
                pavis_core::UpstreamCa::File { path } => {
                    if let Some(ca_bundle) = cluster.ca_bundle() {
                        tracing::debug!(
                            upstream = %upstream_name.0,
                            ca_path = %path.0,
                            ca_count = ca_bundle.len(),
                            "Setting custom CA bundle for upstream TLS verification"
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
                _ => {}
            }
        }

        match upstream.protocol {
            HttpVersion::H1 => peer.options.set_http_version(1, 1),
            HttpVersion::H2 => peer.options.set_http_version(2, 2),
            HttpVersion::H2H1 => peer.options.set_http_version(2, 1),
            #[allow(unreachable_patterns)]
            _ => peer.options.set_http_version(1, 1),
        }

        peer.options.idle_timeout = match upstream.pool.idle {
            pavis_core::IdleTimeout::Disabled => None,
            pavis_core::IdleTimeout::Enabled(duration) => {
                Some(Duration::from_millis(duration.0.get() as u64))
            }
            #[allow(unreachable_patterns)]
            _ => None,
        };
        peer.options.connection_timeout = match upstream.pool.connect {
            pavis_core::ConnectTimeout::Disabled => None,
            pavis_core::ConnectTimeout::Enabled(duration) => {
                Some(Duration::from_millis(duration.0.get() as u64))
            }
            #[allow(unreachable_patterns)]
            _ => None,
        };
        let per_try_timeout = resolve_per_try_timeout(ctx.route_timeout, &ctx.retry_policy);
        peer.options.read_timeout = per_try_timeout;
        peer.options.write_timeout = per_try_timeout;

        // Apply TCP tuning parameters
        if let Some(keepalive_duration) = upstream.pool.tcp_keepalive {
            let keepalive_ms = keepalive_duration.0.get() as u64;
            peer.options.tcp_keepalive = Some(pingora::protocols::TcpKeepalive {
                idle: Duration::from_millis(keepalive_ms),
                interval: Duration::from_millis(keepalive_ms / 3), // RFC 1122 recommends interval < idle
                count: 3,
                #[cfg(target_os = "linux")]
                user_timeout: Duration::from_secs(0), // 0 = use system default per Pingora docs
            });
        }

        // Note: tcp_nodelay is not directly exposed in Pingora v0.6.0 PeerOptions
        // It's controlled at the socket level via upstream_tcp_sock_tweak_hook
        // For now, we log the configured value but don't apply it
        if let Some(nodelay) = upstream.pool.tcp_nodelay
            && !nodelay
        {
            tracing::warn!(
                upstream = %upstream_name.0,
                "TCP_NODELAY explicitly disabled in config, but not supported by Pingora v0.6.0 PeerOptions"
            );
        }

        if let Some(buffer_size) = upstream.pool.recv_buffer_size {
            peer.options.tcp_recv_buf = Some(buffer_size as usize);
        }

        // Log effective upstream configuration (once per upstream, using atomic counter to avoid Mutex)
        static LOGGED_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let log_key = format!("{}:{}", upstream_name.0, cluster as *const _ as usize);
        let hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            log_key.hash(&mut hasher);
            hasher.finish()
        };

        // Use a simple probabilistic approach: only log if this is likely the first time
        if LOGGED_COUNTER.fetch_add(1, Ordering::Relaxed) % 1000 < 10 || hash % 1000 == 0 {
            tracing::info!(
                upstream = %upstream_name.0,
                idle_timeout_ms = ?peer.options.idle_timeout.map(|d| d.as_millis()),
                connection_timeout_ms = ?peer.options.connection_timeout.map(|d| d.as_millis()),
                read_timeout_ms = ?peer.options.read_timeout.map(|d| d.as_millis()),
                write_timeout_ms = ?peer.options.write_timeout.map(|d| d.as_millis()),
                tcp_keepalive_idle_ms = ?peer.options.tcp_keepalive.as_ref().map(|k| k.idle.as_millis()),
                tcp_keepalive_interval_ms = ?peer.options.tcp_keepalive.as_ref().map(|k| k.interval.as_millis()),
                tcp_recv_buf = ?peer.options.tcp_recv_buf,
                tcp_fast_open = peer.options.tcp_fast_open,
                max_connections = upstream.pool.max.0.get(),
                queue_capacity = upstream.pool.queue.capacity,
                queue_timeout_ms = upstream.pool.queue.timeout_ms,
                "Effective upstream configuration"
            );
        }

        Ok(peer)
    }
}

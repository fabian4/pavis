use crate::proxy::context::{RouterContext, TracingSpan};
use crate::proxy::header_ops::{apply_request_headers, apply_response_headers};
use crate::state::RuntimeStateHandle;
use crate::telemetry::Telemetry;
use async_trait::async_trait;
use http::Uri;
use pavis_core::{
    ConnectTimeout, Discovery, EndpointAddr, HeadersPolicy, Hostname, HttpVersion, PathMatch,
    Principal, RouteAction,
};
use pingora::http::RequestHeader;
use pingora::http::ResponseHeader;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use rand::Rng;
use std::borrow::Cow;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::Duration;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub struct Proxy {
    pub state: Arc<RuntimeStateHandle>,
    pub telemetry: Arc<Telemetry>,
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

fn generate_request_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let random_val: u32 = rand::rng().random();
    format!("req-{}-{}", now, random_val)
}

fn apply_route_headers(ctx: &mut RouterContext, route: &pavis_core::Route) {
    ctx.request_headers = route.request_headers.clone();
    ctx.response_headers = route.response_headers.clone();
}

fn calculate_path_rewrite(
    route: &pavis_core::Route,
    uri_path: &str,
    uri_query: Option<&str>,
) -> Option<Uri> {
    match &route.rewrite.path {
        pavis_core::RewritePath::Disabled => None,
        pavis_core::RewritePath::Prefix { from: _, to } => {
            let new_path = match &route.matcher {
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
                    if matches!(route.matcher, PathMatch::Regex { .. }) {
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

fn is_authorized(principal: &Principal, client_identity: Option<&str>) -> bool {
    match principal {
        Principal::Any => true,
        Principal::Authenticated { spiffe } => {
            client_identity.is_some_and(|identity| identity == spiffe.as_str())
        }
        Principal::Prefix { prefix } => {
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
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: std::time::Instant::now(),
            client_identity: None,
            rbac_denied: false,
            upstream_timing: crate::proxy::context::UpstreamTiming::NotStarted,
            route_pattern: crate::proxy::context::RoutePattern::NotMatched,
            req_id: generate_request_id(),
            span: crate::proxy::context::TracingSpan::Disabled,
            runtime_state: None,
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

        // O(1) lookup using Manager
        // Use PINNED state when available, otherwise fall back to latest snapshot (e.g., tests)
        let state = match ctx.runtime_state.clone() {
            Some(state) => state,
            None => {
                tracing::debug!("Runtime state missing from context; using latest snapshot");
                self.state.load()
            }
        };
        let cluster = match state.upstream_manager.get(upstream_name.0.as_str()) {
            Some(u) => u,
            None => return Error::e_explain(InternalError, "Upstream not found in config"),
        };

        let endpoint = match cluster.select_endpoint() {
            Some(e) => e,
            None => return Error::e_explain(InternalError, "Upstream has no endpoints"),
        };

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

        if matches!(ca, Some(pavis_core::UpstreamCa::File { .. })) {
            let ca_bundle = match cluster.ca_bundle() {
                Some(bundle) => bundle,
                None => {
                    return Error::e_explain(InternalError, "Upstream CA bundle not loaded");
                }
            };
            peer.options.ca = Some(ca_bundle);
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

        // Track upstream timing
        ctx.start_upstream();

        Ok(Box::new(peer))
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        if let Some(metrics) = &self.telemetry.metrics {
            metrics.increment_active_connections();
        }

        let req_header = session.req_header();
        let host_header = req_header.headers.get("Host").and_then(|h| h.to_str().ok());
        let uri_path = req_header.uri.path();
        let uri_query = req_header.uri.query();

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

        if let Some((vhost, route)) = state.router.match_request(host_header, uri_path) {
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
            if ctx.client_identity.is_none() {
                ctx.client_identity = extract_client_identity(session);
            }
            if !is_authorized(&route.principal, ctx.client_identity.as_deref()) {
                ctx.rbac_denied = true;

                if let TracingSpan::Active(ref span) = ctx.span {
                    span.record("rbac.denied", true);
                    span.record("error", "RBAC denied");
                }

                tracing::info!(
                    host = %vhost.host.0,
                    route = %route_path(route),
                    principal = ?route.principal,
                    "RBAC denied request"
                );
                let _ = session.respond_error(403).await;
                return Ok(true);
            }

            let host_rewrite = match &route.rewrite.host {
                pavis_core::RewriteHost::Literal { host } => Some(host),
                pavis_core::RewriteHost::Disabled => None,
                #[allow(unreachable_patterns)]
                _ => None,
            };

            let path_rewrite = calculate_path_rewrite(route, uri_path, uri_query);

            if let Some(host) = host_rewrite {
                let req_header = session.as_downstream_mut().req_header_mut();
                if let Err(err) = req_header.insert_header("Host", host.0.as_str()) {
                    tracing::warn!(error = %err, host = %host.0, "Failed to apply host rewrite");
                } else {
                    ctx.sni_override = Some(host.clone());
                }
            }

            if let Some(uri) = path_rewrite {
                session.as_downstream_mut().req_header_mut().set_uri(uri);
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

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
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

    async fn logging(&self, session: &mut Session, _e: Option<&Error>, ctx: &mut Self::CTX) {
        self.telemetry.access_log.log(session, ctx).await;

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

        #[allow(clippy::collapsible_if)]
        if let TracingSpan::Active(ref span) = ctx.span {
            if let Some(response) = session.response_written() {
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
}

fn route_path(route: &pavis_core::Route) -> &str {
    match &route.matcher {
        PathMatch::Prefix { path } => path.0.as_str(),
        PathMatch::Exact { path } => path.0.as_str(),
        PathMatch::Regex { path } => path.0.as_str(),
        #[allow(unreachable_patterns)]
        &_ => "",
    }
}

#[cfg(test)]
mod service_tests;

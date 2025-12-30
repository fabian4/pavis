use crate::proxy::context::RouterContext;
use crate::proxy::header_ops::{apply_request_headers, apply_response_headers};
use crate::state::RuntimeStateHandle;
use crate::telemetry::Telemetry;
use async_trait::async_trait;
use pavis_core::HttpVersion;
use pingora::http::RequestHeader;
use pingora::http::ResponseHeader;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use rand::Rng;
use std::sync::Arc;
use std::time::Duration;

pub struct Proxy {
    pub state: Arc<RuntimeStateHandle>,
    pub telemetry: Arc<Telemetry>,
}

impl Proxy {}

fn apply_route_headers(ctx: &mut RouterContext, route: &pavis_core::Route) {
    ctx.request_headers = route.request_headers.clone();
    ctx.response_headers = route.response_headers.clone();
}

#[async_trait]
impl ProxyHttp for Proxy {
    type CTX = RouterContext;

    fn new_ctx(&self) -> Self::CTX {
        RouterContext {
            upstream_name: None,
            request_headers: None,
            response_headers: None,
            start_time: std::time::Instant::now(),
        }
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
        let state = self.state.load();
        let cluster = match state.upstream_manager.get(upstream_name) {
            Some(u) => u,
            None => return Error::e_explain(InternalError, "Upstream not found in config"),
        };

        let endpoint = match cluster.select_endpoint() {
            Some(e) => e,
            None => return Error::e_explain(InternalError, "Upstream has no endpoints"),
        };

        let upstream = &cluster.config;

        let addr = std::net::SocketAddr::new(endpoint.ip, endpoint.port);

        tracing::debug!(
            upstream = %upstream_name,
            endpoint = %addr,
            lb = ?upstream.load_balancer,
            http_version = ?upstream.http_version,
            "forwarding request"
        );

        let tls_config = upstream.tls.as_ref();
        let use_tls = tls_config.map(|c| c.enabled).unwrap_or(false);
        let sni = tls_config
            .and_then(|c| c.sni.clone())
            .unwrap_or_else(|| "localhost".to_string());

        let mut peer = HttpPeer::new(addr, use_tls, sni);

        if let Some(c) = tls_config {
            peer.options.verify_hostname = c.verify_hostname;
            peer.options.verify_cert = c.verify_cert;
        }

        // Configure HTTP version
        match upstream.http_version {
            HttpVersion::H1 => peer.options.set_http_version(1, 1),
            HttpVersion::H2 => peer.options.set_http_version(2, 2),
            HttpVersion::H2H1 => peer.options.set_http_version(2, 1),
        }

        // Configure connection pooling
        peer.options.idle_timeout = Some(Duration::from_secs(
            upstream.connection_pool.idle_timeout_secs,
        ));
        peer.options.connection_timeout = Some(Duration::from_secs(
            upstream.connection_pool.connection_timeout_secs,
        ));

        Ok(Box::new(peer))
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        let req_header = session.req_header();
        let host_header = req_header.headers.get("Host").and_then(|h| h.to_str().ok());
        let uri_path = req_header.uri.path();

        tracing::debug!(
            method = %req_header.method,
            path = %uri_path,
            host = ?host_header,
            "incoming request"
        );

        let state = self.state.load();
        if let Some((vhost, route)) = state.router.match_request(host_header, uri_path) {
            tracing::trace!(host = %vhost.host, path = %route.path, "matched route");

            let total_weight: u32 = route.destinations.iter().map(|d| d.weight).sum();
            if total_weight == 0 {
                return Ok(false);
            }

            let mut rng = rand::rng();
            let mut pick = rng.random_range(0..total_weight);

            for dest in &route.destinations {
                if pick < dest.weight {
                    ctx.upstream_name = Some(dest.upstream.clone());
                    break;
                }
                pick -= dest.weight;
            }

            apply_route_headers(ctx, route);

            return Ok(false);
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
        apply_request_headers(upstream_request, ctx.request_headers.as_ref())
    }

    fn upstream_response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        apply_response_headers(upstream_response, ctx.response_headers.as_ref())
    }

    async fn logging(&self, session: &mut Session, _e: Option<&Error>, ctx: &mut Self::CTX) {
        self.telemetry
            .access_log
            .log(session, ctx.upstream_name.as_deref(), ctx.start_time)
            .await;
    }
}

#[cfg(test)]
mod service_tests;

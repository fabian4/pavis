use crate::proxy::context::RouterContext;
use crate::proxy::header_ops::{apply_request_headers, apply_response_headers};
use crate::state::RuntimeStateHandle;
use crate::telemetry::Telemetry;
use async_trait::async_trait;
use http::Uri;
use pavis_core::{ConnectTimeout, EndpointAddr, HeadersPolicy, Hostname, HttpVersion, PathMatch};
use pingora::http::RequestHeader;
use pingora::http::ResponseHeader;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use rand::Rng;
use std::net::SocketAddr;
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

fn apply_rewrite(
    session: &mut Session,
    ctx: &mut RouterContext,
    route: &pavis_core::Route,
    uri_path: &str,
    uri_query: Option<&str>,
) {
    let req_header = session.as_downstream_mut().req_header_mut();

    match &route.rewrite.host {
        pavis_core::RewriteHost::Disabled => {}
        pavis_core::RewriteHost::Literal { host } => {
            if let Err(err) = req_header.insert_header("Host", host.0.as_str()) {
                tracing::warn!(error = %err, host = %host.0, "Failed to apply host rewrite");
            } else {
                ctx.sni_override = Some(host.clone());
            }
        }
    };

    match &route.rewrite.path {
        pavis_core::RewritePath::Disabled => {}
        pavis_core::RewritePath::Prefix { from: _, to } => {
            let new_path = match &route.matcher {
                PathMatch::Prefix { path } => uri_path
                    .strip_prefix(path.0.as_str())
                    .map(|suffix| format!("{}{suffix}", to.0)),
                PathMatch::Exact { path } => (uri_path == path.0.as_str()).then(|| to.0.clone()),
                PathMatch::Regex { .. } => None,
            };

            match new_path {
                Some(mut path) => {
                    if let Some(query) = uri_query {
                        path.push('?');
                        path.push_str(query);
                    }

                    match Uri::builder().path_and_query(path.as_str()).build() {
                        Ok(uri) => req_header.set_uri(uri),
                        Err(err) => {
                            tracing::warn!(
                                error = %err,
                                rewrite = %to.0,
                                "Failed to apply path rewrite"
                            );
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
                }
            }
        }
    };
}

#[async_trait]
impl ProxyHttp for Proxy {
    type CTX = RouterContext;

    fn new_ctx(&self) -> Self::CTX {
        RouterContext {
            upstream_name: None,
            request_headers: HeadersPolicy::Disabled,
            response_headers: HeadersPolicy::Disabled,
            sni_override: None,
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
        let cluster = match state.upstream_manager.get(upstream_name.0.as_str()) {
            Some(u) => u,
            None => return Error::e_explain(InternalError, "Upstream not found in config"),
        };

        let endpoint = match cluster.select_endpoint() {
            Some(e) => e,
            None => return Error::e_explain(InternalError, "Upstream has no endpoints"),
        };

        let upstream = &cluster.config;

        let addr = match &endpoint.address {
            EndpointAddr::Ip { address, port } => SocketAddr::new(*address, port.0.get()),
            EndpointAddr::Dns { host, port } => {
                return Error::e_explain(
                    InternalError,
                    format!("DNS upstream {}:{} not supported yet", host.0, port.0),
                );
            }
        };

        tracing::debug!(
            upstream = %upstream_name.0,
            endpoint = %addr,
            lb = ?upstream.balancer,
            http = ?upstream.protocol,
            "forwarding request"
        );

        let (use_tls, sni, verify_mode) = match &upstream.tls {
            pavis_core::TlsPolicy::Disabled => (false, None, None),
            pavis_core::TlsPolicy::Enabled { verify_mode, sni } => {
                let sni_value = match sni {
                    pavis_core::SniName::Auto => ctx.sni_override.clone(),
                    pavis_core::SniName::Value(name) => Some(name.clone()),
                };
                (true, sni_value, Some(verify_mode))
            }
        };

        let sni_value = sni
            .or_else(|| ctx.sni_override.clone())
            .unwrap_or_else(|| Hostname("localhost".to_string()));

        let mut peer = HttpPeer::new(addr, use_tls, sni_value.0);

        if let Some(mode) = verify_mode {
            match mode {
                pavis_core::TlsVerify::Disabled => {
                    peer.options.verify_hostname = false;
                    peer.options.verify_cert = false;
                }
                pavis_core::TlsVerify::Cert => {
                    peer.options.verify_hostname = false;
                    peer.options.verify_cert = true;
                }
                pavis_core::TlsVerify::CertAndHost => {
                    peer.options.verify_hostname = true;
                    peer.options.verify_cert = true;
                }
            }
        }

        // Configure HTTP version
        match upstream.protocol {
            HttpVersion::H1 => peer.options.set_http_version(1, 1),
            HttpVersion::H2 => peer.options.set_http_version(2, 2),
            HttpVersion::H2H1 => peer.options.set_http_version(2, 1),
        }

        // Configure connection pooling
        peer.options.idle_timeout = match upstream.pool.idle {
            pavis_core::IdleTimeout::Disabled => None,
            pavis_core::IdleTimeout::Enabled(duration) => {
                Some(Duration::from_millis(duration.0.get() as u64))
            }
        };
        peer.options.connection_timeout = match upstream.pool.connect {
            ConnectTimeout::Disabled => None,
            ConnectTimeout::Enabled(duration) => {
                Some(Duration::from_millis(duration.0.get() as u64))
            }
        };

        Ok(Box::new(peer))
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        let req_header = session.req_header();
        let host_header = req_header.headers.get("Host").and_then(|h| h.to_str().ok());
        let uri_path = req_header.uri.path().to_string();
        let uri_query = req_header.uri.query().map(str::to_string);

        tracing::debug!(
            method = %req_header.method,
            path = %uri_path,
            host = ?host_header,
            "incoming request"
        );

        let state = self.state.load();
        if let Some((vhost, route)) = state.router.match_request(host_header, &uri_path) {
            tracing::trace!(host = %vhost.host.0, path = %route_path(route), "matched route");

            apply_route_headers(ctx, route);
            apply_rewrite(session, ctx, route, &uri_path, uri_query.as_deref());

            let total_weight: u32 = route
                .destinations
                .iter()
                .map(|d| d.weight.0.get() as u32)
                .sum();
            if total_weight == 0 {
                return Ok(false);
            }

            let mut rng = rand::rng();
            let mut pick = rng.random_range(0..total_weight);

            for dest in &route.destinations {
                let weight = dest.weight.0.get() as u32;
                if pick < weight {
                    ctx.upstream_name = Some(dest.upstream.clone());
                    break;
                }
                pick -= weight;
            }

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
        self.telemetry
            .access_log
            .log(
                session,
                ctx.upstream_name.as_ref().map(|name| name.0.as_str()),
                ctx.start_time,
            )
            .await;
    }
}

fn route_path(route: &pavis_core::Route) -> &str {
    match &route.matcher {
        PathMatch::Prefix { path } => path.0.as_str(),
        PathMatch::Exact { path } => path.0.as_str(),
        PathMatch::Regex { path } => path.0.as_str(),
    }
}

#[cfg(test)]
mod service_tests;

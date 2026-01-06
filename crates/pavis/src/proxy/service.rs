use crate::proxy::context::RouterContext;
use crate::proxy::header_ops::{apply_request_headers, apply_response_headers};
use crate::state::RuntimeStateHandle;
use crate::telemetry::Telemetry;
use async_trait::async_trait;
use http::Uri;
use pavis_core::{
    ConnectTimeout, EndpointAddr, HeadersPolicy, Hostname, HttpVersion, PathMatch, RouteAction,
};
use pingora::http::RequestHeader;
use pingora::http::ResponseHeader;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use rand::Rng;
use std::borrow::Cow;
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
    }
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
            client_identity: None,
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

        let (use_tls, sni, verify_mode, cert) = match &upstream.tls {
            pavis_core::TlsPolicy::Disabled => (false, None, None, None),
            pavis_core::TlsPolicy::Enabled { mode, sni, cert } => {
                let sni_value = match sni {
                    pavis_core::SniName::Auto => ctx.sni_override.clone(),
                    pavis_core::SniName::Value(name) => Some(name.clone()),
                };
                (true, sni_value, Some(mode), Some(cert))
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

        // Configure client certificate for outbound mTLS
        if let Some(cert_config) = cert {
            match cert_config {
                pavis_core::ClientCert::Disabled => {
                    // No client certificate
                }
                pavis_core::ClientCert::Enabled {
                    cert_path,
                    key_path,
                } => {
                    // TODO: Load and configure client certificate for upstream connection
                    // This allows the sidecar to authenticate itself to the upstream service
                    tracing::debug!(
                        cert_path = %cert_path.0,
                        key_path = %key_path.0,
                        "Configuring client certificate for upstream connection"
                    );
                    // peer.options.set_client_cert(&cert_path.0, &key_path.0)?;
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
        let uri_path = req_header.uri.path();
        let uri_query = req_header.uri.query();

        tracing::debug!(
            method = %req_header.method,
            path = %uri_path,
            host = ?host_header,
            "incoming request"
        );

        let state = self.state.load();
        if let Some((vhost, route)) = state.router.match_request(host_header, uri_path) {
            tracing::trace!(host = %vhost.host.0, path = %route_path(route), "matched route");

            apply_route_headers(ctx, route);

            let host_rewrite = match &route.rewrite.host {
                pavis_core::RewriteHost::Literal { host } => Some(host),
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
                            break;
                        }
                        pick -= weight;
                    }
                    return Ok(false);
                }
                RouteAction::Redirect { status, location } => {
                    let status_code = *status;
                    let location_url = location.clone();
                    drop(state);

                    let mut resp = ResponseHeader::build(status_code, None)?;
                    resp.insert_header("Location", location_url.as_str())?;
                    session.write_response_header(Box::new(resp), true).await?;
                    return Ok(true);
                }
                RouteAction::Direct { status, body } => {
                    let status_code = *status;
                    let body_content = body.clone();
                    drop(state);

                    let mut resp = ResponseHeader::build(status_code, None)?;
                    resp.insert_header("Content-Type", "text/plain")?;
                    resp.insert_header("Content-Length", body_content.len().to_string())?;
                    session.write_response_header(Box::new(resp), false).await?;
                    session
                        .write_response_body(Some(body_content.into_bytes().into()), true)
                        .await?;
                    return Ok(true);
                }
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

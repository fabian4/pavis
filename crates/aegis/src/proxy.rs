use crate::config::{AegisConfig, HeaderOperations};
use async_trait::async_trait;
use http::header::{HeaderName, HeaderValue};
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use rand::Rng;
use std::str::FromStr;
use std::sync::Arc;

pub struct MyProxy {
    pub config: Arc<AegisConfig>,
}

pub struct RouterContext {
    pub upstream_name: Option<String>,
    pub matched_headers: Option<HeaderOperations>,
}

#[async_trait]
impl ProxyHttp for MyProxy {
    type CTX = RouterContext;

    fn new_ctx(&self) -> Self::CTX {
        RouterContext {
            upstream_name: None,
            matched_headers: None,
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

        let upstream = self
            .config
            .upstreams
            .iter()
            .find(|u| &u.name == upstream_name);

        if let Some(u) = upstream {
            if u.endpoints.is_empty() {
                return Error::e_explain(InternalError, "Upstream has no endpoints");
            }
            let mut rng = rand::rng();
            let idx = rng.random_range(0..u.endpoints.len());
            let endpoint = &u.endpoints[idx];

            let addr = format!("{}:{}", endpoint.ip, endpoint.port);
            tracing::info!("Forwarding to upstream: {} -> {}", upstream_name, addr);

            let peer = Box::new(HttpPeer::new(
                &addr,
                false, // TLS disabled for now
                "localhost".to_string(),
            ));
            return Ok(peer);
        }

        Error::e_explain(InternalError, "Upstream not found in config")
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        let req_header = session.req_header();
        let host_header = req_header.headers.get("Host").and_then(|h| h.to_str().ok());
        let uri_path = req_header.uri.path();

        tracing::info!(
            "Incoming request: method={}, path={}, host={:?}",
            req_header.method,
            uri_path,
            host_header
        );

        for vhost in &self.config.routes {
            if vhost.host == "*" || Some(vhost.host.as_str()) == host_header {
                for route in vhost.paths.iter() {
                    let is_match = match route.match_type.as_str() {
                        "prefix" => uri_path.starts_with(&route.path),
                        "exact" => uri_path == route.path,
                        _ => false,
                    };

                    if is_match {
                        tracing::debug!("Matched route: host={}, path={}", vhost.host, route.path);

                        let total_weight: u32 = route.destinations.iter().map(|d| d.weight).sum();
                        if total_weight == 0 {
                            continue;
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

                        if let Some(headers) = &route.headers {
                            ctx.matched_headers = Some(headers.clone());
                        }

                        return Ok(false);
                    }
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
        upstream_request.insert_header("X-Proxy-By", "Aegis")?;

        if let Some(headers) = &ctx.matched_headers {
            if let Some(add_map) = &headers.add {
                for (k, v) in add_map {
                    if let Ok(val) = HeaderValue::from_str(v) {
                        if let Ok(key) = HeaderName::from_str(k) {
                            upstream_request.insert_header(key, val)?;
                        }
                    }
                }
            }
            if let Some(remove_list) = &headers.remove {
                for k in remove_list {
                    upstream_request.remove_header(k);
                }
            }
        }

        Ok(())
    }
}

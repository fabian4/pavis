use crate::config::{HeaderOperations, PavisConfig, Route, VirtualHost};
use async_trait::async_trait;
use http::header::{HeaderName, HeaderValue};
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use rand::Rng;
use std::str::FromStr;
use std::sync::Arc;

pub struct MyProxy {
    pub config: Arc<PavisConfig>,
}

pub struct RouterContext {
    pub upstream_name: Option<String>,
    pub matched_headers: Option<HeaderOperations>,
}

pub fn find_route<'a>(
    config: &'a PavisConfig,
    host_header: Option<&str>,
    uri_path: &str,
) -> Option<(&'a VirtualHost, &'a Route)> {
    for vhost in &config.routes {
        if vhost.host == "*" || Some(vhost.host.as_str()) == host_header {
            for route in vhost.paths.iter() {
                let is_match = match route.match_type.as_str() {
                    "prefix" => uri_path.starts_with(&route.path),
                    "exact" => uri_path == route.path,
                    _ => false,
                };

                if is_match {
                    return Some((vhost, route));
                }
            }
        }
    }
    None
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

        if let Some((vhost, route)) = find_route(&self.config, host_header, uri_path) {
            tracing::debug!("Matched route: host={}, path={}", vhost.host, route.path);

            let total_weight: u32 = route.destinations.iter().map(|d| d.weight).sum();
            if total_weight == 0 {
                return Ok(false); // Or handle as error/no-op?
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

        let _ = session.respond_error(404).await;
        Ok(true)
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        upstream_request.insert_header("X-Proxy-By", "Pavis")?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Route, VirtualHost, WeightedDestination};

    fn create_test_config() -> PavisConfig {
        PavisConfig {
            server: crate::config::ServerConfig {
                listen_addr: "0.0.0.0:8080".to_string(),
                worker_threads: None,
                tls: None,
            },
            telemetry: crate::config::TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: None,
                tracing: None,
            },
            upstreams: vec![],
            routes: vec![
                VirtualHost {
                    host: "example.com".to_string(),
                    paths: vec![
                        Route {
                            match_type: "exact".to_string(),
                            path: "/exact".to_string(),
                            timeout_ms: None,
                            retry: None,
                            headers: None,
                            destinations: vec![WeightedDestination {
                                upstream: "backend-1".to_string(),
                                weight: 1,
                            }],
                        },
                        Route {
                            match_type: "prefix".to_string(),
                            path: "/api".to_string(),
                            timeout_ms: None,
                            retry: None,
                            headers: None,
                            destinations: vec![WeightedDestination {
                                upstream: "backend-1".to_string(),
                                weight: 1,
                            }],
                        },
                    ],
                },
                VirtualHost {
                    host: "*".to_string(),
                    paths: vec![Route {
                        match_type: "prefix".to_string(),
                        path: "/public".to_string(),
                        timeout_ms: None,
                        retry: None,
                        headers: None,
                        destinations: vec![WeightedDestination {
                            upstream: "backend-2".to_string(),
                            weight: 1,
                        }],
                    }],
                },
            ],
        }
    }

    #[test]
    fn test_find_route_exact_match() {
        let config = create_test_config();
        let (vhost, route) =
            find_route(&config, Some("example.com"), "/exact").expect("Should match");
        assert_eq!(vhost.host, "example.com");
        assert_eq!(route.path, "/exact");
    }

    #[test]
    fn test_find_route_prefix_match() {
        let config = create_test_config();
        let (vhost, route) =
            find_route(&config, Some("example.com"), "/api/v1/users").expect("Should match");
        assert_eq!(vhost.host, "example.com");
        assert_eq!(route.path, "/api");
    }

    #[test]
    fn test_find_route_wildcard_host() {
        let config = create_test_config();
        let (vhost, route) =
            find_route(&config, Some("any.com"), "/public/stuff").expect("Should match");
        assert_eq!(vhost.host, "*");
        assert_eq!(route.path, "/public");
    }

    #[test]
    fn test_find_route_no_match() {
        let config = create_test_config();
        let result = find_route(&config, Some("example.com"), "/notfound");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_route_wrong_host() {
        let config = create_test_config();
        // "other.com" matches "*" host but path "/exact" is only on "example.com"
        // Wait, "*" host has "/public". "/exact" is NOT on "*".
        let result = find_route(&config, Some("other.com"), "/exact");
        assert!(result.is_none());
    }
}

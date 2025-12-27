//! Proxy module: The runtime coordination layer.
//!
//! # Architectural Invariants
//!
//! 1. **No Business Logic**: This module orchestrates `router`, `upstream`, and `telemetry`.
//!    It should not contain complex logic for matching or load balancing.
//! 2. **Non-Blocking**: All operations must be async and non-blocking.
//!    - No `std::sync::Mutex` (use `tokio::sync::Mutex` if absolutely necessary, but prefer lock-free).
//!    - No blocking I/O (file, network).
//! 3. **No Mutable Global State**: State should be encapsulated in components (`Router`, `Manager`).
//! 4. **Validated Configuration**: The proxy assumes configuration is valid and immutable.

use crate::router::Router;
use crate::telemetry::Telemetry;
use crate::upstream::Manager;
use async_trait::async_trait;
use http::header::{HeaderName, HeaderValue};
use pavis_core::config::{HeaderOperations, HttpVersion};
use pingora::http::ResponseHeader;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use rand::Rng;
use std::str::FromStr;
use std::sync::Arc;

pub struct Proxy {
    pub router: Arc<Router>,
    pub upstream_manager: Manager,
    pub telemetry: Arc<Telemetry>,
}

pub struct RouterContext {
    pub upstream_name: Option<String>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub start_time: std::time::Instant,
}

pub fn apply_request_headers(
    req: &mut RequestHeader,
    headers: Option<&HeaderOperations>,
) -> Result<()> {
    req.insert_header("X-Proxy-By", "Pavis")?;

    if let Some(headers) = headers {
        if let Some(add_map) = &headers.add {
            for (k, v) in add_map {
                match (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                    (Ok(key), Ok(val)) => {
                        req.insert_header(key, val)?;
                    }
                    (Err(e), _) => {
                        tracing::warn!("Invalid request header name '{:?}': {}", k, e);
                    }
                    (_, Err(e)) => {
                        tracing::warn!("Invalid request header value for '{:?}': {}", k, e);
                    }
                }
            }
        }
        if let Some(remove_list) = &headers.remove {
            for k in remove_list {
                req.remove_header(k);
            }
        }
    }
    Ok(())
}

pub fn apply_response_headers(
    resp: &mut ResponseHeader,
    headers: Option<&HeaderOperations>,
) -> Result<()> {
    resp.insert_header("X-Proxy-By", "Pavis")?;

    if let Some(headers) = headers {
        if let Some(add_map) = &headers.add {
            for (k, v) in add_map {
                match (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                    (Ok(key), Ok(val)) => {
                        resp.insert_header(key, val)?;
                    }
                    (Err(e), _) => {
                        tracing::warn!("Invalid response header name '{:?}': {}", k, e);
                    }
                    (_, Err(e)) => {
                        tracing::warn!("Invalid response header value for '{:?}': {}", k, e);
                    }
                }
            }
        }
        if let Some(remove_list) = &headers.remove {
            for k in remove_list {
                resp.remove_header(k);
            }
        }
    }
    Ok(())
}

impl Proxy {}

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
        let cluster = match self.upstream_manager.get(upstream_name) {
            Some(u) => u,
            None => return Error::e_explain(InternalError, "Upstream not found in config"),
        };

        let endpoint = match cluster.select_endpoint() {
            Some(e) => e,
            None => return Error::e_explain(InternalError, "Upstream has no endpoints"),
        };

        let upstream = &cluster.config;

        let addr = endpoint.address();

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
            if let Some(verify) = c.verify_hostname {
                peer.options.verify_hostname = verify;
            }
            if let Some(verify) = c.verify_cert {
                peer.options.verify_cert = verify;
            }
        }

        // Configure HTTP version
        match upstream.http_version {
            HttpVersion::H1 => peer.options.set_http_version(1, 1),
            HttpVersion::H2 => peer.options.set_http_version(2, 2),
            HttpVersion::H2H1 => peer.options.set_http_version(2, 1),
        }

        // Configure connection pooling
        peer.options.idle_timeout = Some(upstream.connection_pool.idle_timeout);
        peer.options.connection_timeout = Some(upstream.connection_pool.connection_timeout);

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

        if let Some((vhost, route)) = self.router.match_request(host_header, uri_path) {
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

            ctx.request_headers = route.request_headers.clone();
            ctx.response_headers = route.response_headers.clone();

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
mod tests {
    use super::*;
    use crate::upstream::Cluster;
    use pavis_core::config::{
        LoadBalancer, MatchType, RawConfig, Route, VirtualHost, WeightedDestination,
    };
    use std::collections::HashMap;

    fn create_test_config() -> RawConfig {
        RawConfig {
            server: pavis_core::config::ServerConfig {
                listen_addr: "0.0.0.0:8080".to_string(),
                worker_threads: None,
                tls: None,
            },
            telemetry: pavis_core::config::TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: pavis_core::config::AccessLogConfig::False,
                tracing: None,
            },
            upstreams: vec![],
            routes: vec![
                VirtualHost {
                    host: "example.com".to_string(),
                    paths: vec![
                        Route {
                            match_type: MatchType::Exact,
                            path: "/exact".to_string(),
                            timeout: None,
                            retry: None,
                            request_headers: None,
                            response_headers: None,
                            destinations: vec![WeightedDestination {
                                upstream: "backend-1".to_string(),
                                weight: 1,
                            }],
                            compiled_regex: None,
                        },
                        Route {
                            match_type: MatchType::Prefix,
                            path: "/api".to_string(),
                            timeout: None,
                            retry: None,
                            request_headers: None,
                            response_headers: None,
                            destinations: vec![WeightedDestination {
                                upstream: "backend-1".to_string(),
                                weight: 1,
                            }],
                            compiled_regex: None,
                        },
                    ],
                },
                VirtualHost {
                    host: "*".to_string(),
                    paths: vec![Route {
                        match_type: MatchType::Prefix,
                        path: "/public".to_string(),
                        timeout: None,
                        retry: None,
                        request_headers: None,
                        response_headers: None,
                        destinations: vec![WeightedDestination {
                            upstream: "backend-2".to_string(),
                            weight: 1,
                        }],
                        compiled_regex: None,
                    }],
                },
            ],
        }
    }

    #[test]
    fn test_find_route_exact_match() {
        let config = create_test_config();
        let router = Router::new(&config.routes).unwrap();
        let (vhost, route) = router
            .match_request(Some("example.com"), "/exact")
            .expect("Should match");
        assert_eq!(vhost.host, "example.com");
        assert_eq!(route.path, "/exact");
    }

    #[test]
    fn test_find_route_prefix_match() {
        let config = create_test_config();
        let router = Router::new(&config.routes).unwrap();
        let (vhost, route) = router
            .match_request(Some("example.com"), "/api/v1/users")
            .expect("Should match");
        assert_eq!(vhost.host, "example.com");
        assert_eq!(route.path, "/api");
    }

    #[test]
    fn test_find_route_wildcard_host() {
        let config = create_test_config();
        let router = Router::new(&config.routes).unwrap();
        let (vhost, route) = router
            .match_request(Some("any.com"), "/public/stuff")
            .expect("Should match");
        assert_eq!(vhost.host, "*");
        assert_eq!(route.path, "/public");
    }

    #[test]
    fn test_find_route_no_match() {
        let config = create_test_config();
        let router = Router::new(&config.routes).unwrap();
        let result = router.match_request(Some("example.com"), "/notfound");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_route_wrong_host() {
        let config = create_test_config();
        let router = Router::new(&config.routes).unwrap();
        // "other.com" matches "*" host but path "/exact" is only on "example.com"
        // Wait, "*" host has "/public". "/exact" is NOT on "*".
        let result = router.match_request(Some("other.com"), "/exact");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_route_regex_match() {
        let config = RawConfig {
            server: pavis_core::config::ServerConfig {
                listen_addr: "0.0.0.0:8080".to_string(),
                worker_threads: None,
                tls: None,
            },
            telemetry: pavis_core::config::TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: pavis_core::config::AccessLogConfig::False,
                tracing: None,
            },
            upstreams: vec![],
            routes: vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Regex,
                    path: r"^/api/v[0-9]+/users/\d+$".to_string(),
                    timeout: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend".to_string(),
                        weight: 1,
                    }],
                    compiled_regex: None,
                }],
            }],
        };

        let router = Router::new(&config.routes).unwrap();

        // Should match
        let result = router.match_request(None, "/api/v1/users/123");
        assert!(result.is_some());
        let (_, route) = result.unwrap();
        assert_eq!(route.match_type, MatchType::Regex);
        assert!(route.compiled_regex.is_some());

        // Should match v2
        let result = router.match_request(None, "/api/v2/users/456");
        assert!(result.is_some());

        // Should NOT match (missing user id)
        let result = router.match_request(None, "/api/v1/users/");
        assert!(result.is_none());

        // Should NOT match (non-numeric user id)
        let result = router.match_request(None, "/api/v1/users/abc");
        assert!(result.is_none());
    }

    #[test]
    fn test_weighted_round_robin_respects_weights() {
        let upstream = pavis_core::config::Upstream {
            name: "test".to_string(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: pavis_core::config::HttpVersion::H1,
            connection_pool: pavis_core::config::ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![
                pavis_core::config::Endpoint {
                    ip: "A".to_string(),
                    port: 8080,
                    weight: Some(3),
                },
                pavis_core::config::Endpoint {
                    ip: "B".to_string(),
                    port: 8081,
                    weight: Some(1),
                },
            ],
        };

        let cluster = Cluster::new(upstream);

        // Total weight = 4. Pattern should be A, A, A, B
        assert_eq!(cluster.select_endpoint().unwrap().ip, "A"); // 0
        assert_eq!(cluster.select_endpoint().unwrap().ip, "A"); // 1
        assert_eq!(cluster.select_endpoint().unwrap().ip, "A"); // 2
        assert_eq!(cluster.select_endpoint().unwrap().ip, "B"); // 3
        assert_eq!(cluster.select_endpoint().unwrap().ip, "A"); // 4
    }

    #[test]
    fn test_round_robin_cycles_endpoints_evenly() {
        let upstream = pavis_core::config::Upstream {
            name: "test-upstream".to_string(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: pavis_core::config::HttpVersion::H1,
            connection_pool: pavis_core::config::ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![
                pavis_core::config::Endpoint {
                    ip: "127.0.0.1".to_string(),
                    port: 8081,
                    weight: None,
                },
                pavis_core::config::Endpoint {
                    ip: "127.0.0.1".to_string(),
                    port: 8082,
                    weight: None,
                },
                pavis_core::config::Endpoint {
                    ip: "127.0.0.1".to_string(),
                    port: 8083,
                    weight: None,
                },
            ],
        };

        let cluster = Cluster::new(upstream);

        // We can't easily check the index returned by select_endpoint because it returns &Endpoint.
        // But we can check the port or ip.

        let e1 = cluster.select_endpoint().unwrap();
        assert_eq!(e1.port, 8081);

        let e2 = cluster.select_endpoint().unwrap();
        assert_eq!(e2.port, 8082);

        let e3 = cluster.select_endpoint().unwrap();
        assert_eq!(e3.port, 8083);

        let e4 = cluster.select_endpoint().unwrap();
        assert_eq!(e4.port, 8081);
    }

    #[test]
    fn test_concurrent_round_robin() {
        let upstream = pavis_core::config::Upstream {
            name: "concurrent-upstream".to_string(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: pavis_core::config::HttpVersion::H1,
            connection_pool: pavis_core::config::ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![
                pavis_core::config::Endpoint {
                    ip: "A".to_string(),
                    port: 80,
                    weight: None,
                },
                pavis_core::config::Endpoint {
                    ip: "B".to_string(),
                    port: 80,
                    weight: None,
                },
            ],
        };

        let cluster = Arc::new(Cluster::new(upstream));

        let mut handles = vec![];
        for _ in 0..10 {
            let c = cluster.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    let _ = c.select_endpoint();
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let count = cluster
            .rr_counter
            .0
            .load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(count, 1000);
    }
    #[test]
    fn test_apply_headers() {
        let mut req = RequestHeader::build("GET", b"/", None).unwrap();
        // Add a header to be removed
        req.insert_header("X-Remove", "old-value").unwrap();

        let mut add_map = HashMap::new();
        add_map.insert("X-Add".to_string(), "new-value".to_string());

        let remove_list = vec!["X-Remove".to_string()];

        let ops = HeaderOperations {
            add: Some(add_map),
            remove: Some(remove_list),
        };

        apply_request_headers(&mut req, Some(&ops)).unwrap();

        // Check X-Proxy-By (always added)
        assert_eq!(
            req.headers.get("X-Proxy-By").unwrap().to_str().unwrap(),
            "Pavis"
        );

        // Check added header
        assert_eq!(
            req.headers.get("X-Add").unwrap().to_str().unwrap(),
            "new-value"
        );

        // Check removed header
        assert!(req.headers.get("X-Remove").is_none());
    }

    #[test]
    fn test_apply_response_headers() {
        let mut resp = ResponseHeader::build(200, None).unwrap();
        // Add a header to be removed
        resp.insert_header("X-Remove-Resp", "bad-value").unwrap();

        let mut add_map = HashMap::new();
        add_map.insert("X-Add-Resp".to_string(), "good-value".to_string());

        let remove_list = vec!["X-Remove-Resp".to_string()];

        let ops = HeaderOperations {
            add: Some(add_map),
            remove: Some(remove_list),
        };

        apply_response_headers(&mut resp, Some(&ops)).unwrap();

        // Check X-Proxy-By (always added)
        assert_eq!(
            resp.headers.get("X-Proxy-By").unwrap().to_str().unwrap(),
            "Pavis"
        );

        // Check added header
        assert_eq!(
            resp.headers.get("X-Add-Resp").unwrap().to_str().unwrap(),
            "good-value"
        );

        // Check removed header
        assert!(resp.headers.get("X-Remove-Resp").is_none());
    }
}

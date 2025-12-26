use crate::config::{
    AccessLogConfig, Config, HeaderOperations, HttpVersion, LoadBalancer, MatchType, Route,
    Upstream, VirtualHost,
};
use async_trait::async_trait;
use http::header::{HeaderName, HeaderValue};
use pingora::http::ResponseHeader;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use rand::Rng;
use regex::Regex;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Proxy {
    pub config: Arc<Config>,
    /// Pre-built HashMap for O(1) upstream lookup
    pub upstreams: HashMap<String, Upstream>,
    pub upstream_counters: HashMap<String, AtomicUsize>,
}

pub struct RouterContext {
    pub upstream_name: Option<String>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
}

pub fn find_route<'a>(
    config: &'a Config,
    host_header: Option<&str>,
    uri_path: &str,
) -> Option<(&'a VirtualHost, &'a Route)> {
    for vhost in &config.routes {
        if vhost.host == "*" || Some(vhost.host.as_str()) == host_header {
            for route in vhost.paths.iter() {
                let is_match = match route.match_type {
                    MatchType::Prefix => uri_path.starts_with(&route.path),
                    MatchType::Exact => uri_path == route.path,
                    MatchType::Regex => Regex::new(&route.path)
                        .map(|re| re.is_match(uri_path))
                        .unwrap_or(false),
                };

                if is_match {
                    return Some((vhost, route));
                }
            }
        }
    }
    None
}

pub fn apply_request_headers(
    req: &mut RequestHeader,
    headers: Option<&HeaderOperations>,
) -> Result<()> {
    req.insert_header("X-Proxy-By", "Pavis")?;

    if let Some(headers) = headers {
        if let Some(add_map) = &headers.add {
            for (k, v) in add_map {
                if let (Ok(key), Ok(val)) = (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                    req.insert_header(key, val)?;
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
                if let (Ok(key), Ok(val)) = (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                    resp.insert_header(key, val)?;
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

impl Proxy {
    pub fn select_endpoint_index(&self, upstream: &crate::config::Upstream) -> usize {
        if upstream.endpoints.is_empty() {
            return 0;
        }

        let total_weight: u32 = upstream
            .endpoints
            .iter()
            .map(|e| e.weight.unwrap_or(1))
            .sum();

        if total_weight == 0 {
            return 0;
        }

        match upstream.load_balancer {
            LoadBalancer::RoundRobin => {
                let counter = if let Some(c) = self.upstream_counters.get(&upstream.name) {
                    c.fetch_add(1, Ordering::Relaxed)
                } else {
                    let mut rng = rand::rng();
                    return rng.random_range(0..upstream.endpoints.len());
                };

                // Weighted Round Robin logic (Virtual Ring)
                let mut current = (counter as u32) % total_weight;

                for (i, endpoint) in upstream.endpoints.iter().enumerate() {
                    let w = endpoint.weight.unwrap_or(1);
                    if current < w {
                        return i;
                    }
                    current -= w;
                }
                0
            }
            LoadBalancer::Random => {
                // Weighted Random
                let mut rng = rand::rng();
                let mut pick = rng.random_range(0..total_weight);

                for (i, endpoint) in upstream.endpoints.iter().enumerate() {
                    let w = endpoint.weight.unwrap_or(1);
                    if pick < w {
                        return i;
                    }
                    pick -= w;
                }
                0
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
            request_headers: None,
            response_headers: None,
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

        // O(1) lookup using HashMap
        let upstream = match self.upstreams.get(upstream_name) {
            Some(u) => u,
            None => return Error::e_explain(InternalError, "Upstream not found in config"),
        };

        if upstream.endpoints.is_empty() {
            return Error::e_explain(InternalError, "Upstream has no endpoints");
        }

        let idx = self.select_endpoint_index(upstream);
        let endpoint = &upstream.endpoints[idx];

        let addr = format!("{}:{}", endpoint.ip, endpoint.port);
        tracing::debug!(
            upstream = %upstream_name,
            endpoint = %addr,
            lb = ?upstream.load_balancer,
            http_version = ?upstream.http_version,
            "forwarding request"
        );

        let mut peer = HttpPeer::new(
            &addr,
            false, // TLS disabled for now
            "localhost".to_string(),
        );

        // Configure HTTP version
        match upstream.http_version {
            HttpVersion::H1 => peer.options.set_http_version(1, 1),
            HttpVersion::H2 => peer.options.set_http_version(2, 2),
            HttpVersion::H2H1 => peer.options.set_http_version(2, 1),
        }

        // Configure connection pooling
        peer.options.idle_timeout = Some(std::time::Duration::from_secs(
            upstream.connection_pool.idle_timeout_secs,
        ));
        peer.options.connection_timeout = Some(std::time::Duration::from_secs(
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

        if let Some((vhost, route)) = find_route(&self.config, host_header, uri_path) {
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
        match &self.config.telemetry.access_log {
            AccessLogConfig::False => {}
            AccessLogConfig::Stdout | AccessLogConfig::File(_) => {
                let req = session.req_header();
                let method = &req.method;
                let path = req.uri.path();
                let host = req
                    .headers
                    .get("host")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or("-");

                let status = session
                    .response_written()
                    .map(|r| r.status.as_u16())
                    .unwrap_or(0);

                let upstream = ctx.upstream_name.as_deref().unwrap_or("-");

                let log_line = format!(
                    "{} {} {} {} {} {}\n",
                    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
                    method,
                    host,
                    path,
                    status,
                    upstream
                );

                match &self.config.telemetry.access_log {
                    AccessLogConfig::Stdout => {
                        print!("{}", log_line);
                    }
                    AccessLogConfig::File(path) => {
                        if let Ok(mut file) =
                            OpenOptions::new().create(true).append(true).open(path)
                        {
                            let _ = file.write_all(log_line.as_bytes());
                        }
                    }
                    AccessLogConfig::False => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LoadBalancer, MatchType, Route, VirtualHost, WeightedDestination};

    fn create_test_config() -> Config {
        Config {
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
                access_log: crate::config::AccessLogConfig::False,
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
                            timeout_ms: None,
                            retry: None,
                            request_headers: None,
                            response_headers: None,
                            destinations: vec![WeightedDestination {
                                upstream: "backend-1".to_string(),
                                weight: 1,
                            }],
                        },
                        Route {
                            match_type: MatchType::Prefix,
                            path: "/api".to_string(),
                            timeout_ms: None,
                            retry: None,
                            request_headers: None,
                            response_headers: None,
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
                        match_type: MatchType::Prefix,
                        path: "/public".to_string(),
                        timeout_ms: None,
                        retry: None,
                        request_headers: None,
                        response_headers: None,
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

    #[test]
    fn test_find_route_regex_match() {
        let config = Config {
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
                access_log: crate::config::AccessLogConfig::False,
                tracing: None,
            },
            upstreams: vec![],
            routes: vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Regex,
                    path: r"^/api/v[0-9]+/users/\d+$".to_string(),
                    timeout_ms: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend".to_string(),
                        weight: 1,
                    }],
                }],
            }],
        };

        // Should match
        let result = find_route(&config, None, "/api/v1/users/123");
        assert!(result.is_some());
        let (_, route) = result.unwrap();
        assert_eq!(route.match_type, MatchType::Regex);

        // Should match v2
        let result = find_route(&config, None, "/api/v2/users/456");
        assert!(result.is_some());

        // Should NOT match (missing user id)
        let result = find_route(&config, None, "/api/v1/users/");
        assert!(result.is_none());

        // Should NOT match (non-numeric user id)
        let result = find_route(&config, None, "/api/v1/users/abc");
        assert!(result.is_none());
    }

    #[test]
    fn test_weighted_round_robin_respects_weights() {
        let upstream = crate::config::Upstream {
            name: "weighted-upstream".to_string(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: crate::config::HttpVersion::H1,
            connection_pool: crate::config::ConnectionPoolConfig::default(),
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![
                crate::config::Endpoint {
                    ip: "A".to_string(),
                    port: 80,
                    weight: Some(3), // 0, 1, 2
                },
                crate::config::Endpoint {
                    ip: "B".to_string(),
                    port: 80,
                    weight: Some(1), // 3
                },
            ],
        };

        let mut counters = HashMap::new();
        counters.insert(
            "weighted-upstream".to_string(),
            std::sync::atomic::AtomicUsize::new(0),
        );

        let proxy = Proxy {
            config: Arc::new(create_test_config()),
            upstreams: HashMap::new(),
            upstream_counters: counters,
        };

        // Total weight = 4. Pattern should be A, A, A, B
        assert_eq!(proxy.select_endpoint_index(&upstream), 0, "0 -> A");
        assert_eq!(proxy.select_endpoint_index(&upstream), 0, "1 -> A");
        assert_eq!(proxy.select_endpoint_index(&upstream), 0, "2 -> A");
        assert_eq!(proxy.select_endpoint_index(&upstream), 1, "3 -> B");
        assert_eq!(proxy.select_endpoint_index(&upstream), 0, "4 -> A");
    }

    #[test]
    fn test_round_robin_cycles_endpoints_evenly() {
        let upstream = crate::config::Upstream {
            name: "test-upstream".to_string(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: crate::config::HttpVersion::H1,
            connection_pool: crate::config::ConnectionPoolConfig::default(),
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![
                crate::config::Endpoint {
                    ip: "127.0.0.1".to_string(),
                    port: 8081,
                    weight: None,
                },
                crate::config::Endpoint {
                    ip: "127.0.0.1".to_string(),
                    port: 8082,
                    weight: None,
                },
                crate::config::Endpoint {
                    ip: "127.0.0.1".to_string(),
                    port: 8083,
                    weight: None,
                },
            ],
        };

        let mut counters = HashMap::new();
        counters.insert(
            "test-upstream".to_string(),
            std::sync::atomic::AtomicUsize::new(0),
        );

        let proxy = Proxy {
            config: Arc::new(crate::config::Config {
                server: crate::config::ServerConfig {
                    listen_addr: "".to_string(),
                    worker_threads: None,
                    tls: None,
                },
                telemetry: crate::config::TelemetryConfig {
                    level: None,
                    pingora: None,
                    service_name: None,
                    prometheus_addr: None,
                    access_log: crate::config::AccessLogConfig::False,
                    tracing: None,
                },
                upstreams: vec![upstream.clone()],
                routes: vec![],
            }),
            upstreams: HashMap::new(),
            upstream_counters: counters,
        };

        assert_eq!(proxy.select_endpoint_index(&upstream), 0);
        assert_eq!(proxy.select_endpoint_index(&upstream), 1);
        assert_eq!(proxy.select_endpoint_index(&upstream), 2);
        assert_eq!(proxy.select_endpoint_index(&upstream), 0);
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

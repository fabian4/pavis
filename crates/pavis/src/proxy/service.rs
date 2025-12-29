use crate::proxy::context::RouterContext;
use crate::proxy::header_ops::{apply_request_headers, apply_response_headers};
use crate::router::Router;
use crate::telemetry::Telemetry;
use crate::upstream::Manager;
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
    pub router: Arc<Router>,
    pub upstream_manager: Manager,
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
        let cluster = match self.upstream_manager.get(upstream_name) {
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
mod header_tests {
    use super::apply_route_headers;
    use crate::proxy::context::RouterContext;
    use pavis_core::{HeaderOperations, MatchType, Route, WeightedDestination};

    #[test]
    fn apply_route_headers_populates_router_context() {
        let route = Route {
            match_type: MatchType::Exact,
            path: "/".to_string(),
            timeout_ms: None,
            retry_policy: None,
            request_headers: Some(HeaderOperations {
                add: vec![("x-req".to_string(), "1".to_string())],
                remove: vec!["x-remove".to_string()],
            }),
            response_headers: Some(HeaderOperations {
                add: vec![("x-resp".to_string(), "ok".to_string())],
                remove: vec![],
            }),
            destinations: vec![WeightedDestination {
                upstream: "backend".to_string(),
                weight: 1,
            }],
            compiled_regex: None,
        };
        let mut ctx = RouterContext {
            upstream_name: None,
            request_headers: None,
            response_headers: None,
            start_time: std::time::Instant::now(),
        };

        apply_route_headers(&mut ctx, &route);

        assert!(ctx.request_headers.is_some());
        assert!(ctx.response_headers.is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::Proxy;
    use crate::router::Router;
    use crate::telemetry::Telemetry;
    use crate::upstream::Manager;
    use pavis_core::{
        AccessLogConfig, ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, MatchType,
        Route, TelemetryConfig, Upstream, VirtualHost, WeightedDestination,
    };
    use pingora::proxy::ProxyHttp;
    use pingora::proxy::Session;
    use std::collections::HashSet;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Arc;
    use std::time::Instant;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_telemetry() -> Arc<Telemetry> {
        let (telemetry, _worker) = Telemetry::new(&TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            prometheus_addr: None,
            access_log: AccessLogConfig::Disabled,
            tracing: None,
        });
        Arc::new(telemetry)
    }

    #[test]
    fn new_ctx_defaults_are_empty() {
        let router = Arc::new(Router::new(vec![]).expect("empty routes"));
        let manager = Manager::new(&[]);
        let proxy = Proxy {
            router,
            upstream_manager: manager,
            telemetry: test_telemetry(),
        };

        let before = Instant::now();
        let ctx = proxy.new_ctx();
        assert!(ctx.upstream_name.is_none());
        assert!(ctx.request_headers.is_none());
        assert!(ctx.response_headers.is_none());
        assert!(ctx.start_time >= before);
    }

    async fn session_for_request(request: &[u8]) -> (Session, tokio::io::DuplexStream) {
        let (mut client, server) = tokio::io::duplex(1024);
        client.write_all(request).await.expect("write request");
        let mut session = Session::new_h1(Box::new(server));
        session.read_request().await.expect("read request");
        (session, client)
    }

    fn upstream(name: &str, port: u16) -> Upstream {
        Upstream {
            name: name.to_string(),
            load_balancer: LoadBalancer::Random,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig {
                idle_timeout_secs: 60,
                connection_timeout_secs: 5,
            },
            tls: None,
            endpoints: vec![Endpoint {
                ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port,
                weight: 1,
            }],
        }
    }

    #[tokio::test]
    async fn request_filter_selects_weighted_destination() {
        let routes = vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Exact,
                path: "/api".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                destinations: vec![
                    WeightedDestination {
                        upstream: "blue".to_string(),
                        weight: 1,
                    },
                    WeightedDestination {
                        upstream: "green".to_string(),
                        weight: 2,
                    },
                ],
                compiled_regex: None,
            }],
        }];
        let router = Arc::new(Router::new(routes).expect("routes"));
        let manager = Manager::new(&[upstream("blue", 8081), upstream("green", 8082)]);
        let proxy = Proxy {
            router,
            upstream_manager: manager,
            telemetry: test_telemetry(),
        };

        let (mut session, _client) =
            session_for_request(b"GET /api HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
        let mut ctx = proxy.new_ctx();
        let should_respond = proxy
            .request_filter(&mut session, &mut ctx)
            .await
            .expect("request filter");
        assert!(!should_respond);

        let expected: HashSet<&str> = ["blue", "green"].into_iter().collect();
        let selected = ctx.upstream_name.as_deref().expect("upstream selected");
        assert!(expected.contains(selected));
    }

    #[tokio::test]
    async fn request_filter_returns_404_when_no_route_matches() {
        let router = Arc::new(Router::new(vec![]).expect("empty routes"));
        let manager = Manager::new(&[]);
        let proxy = Proxy {
            router,
            upstream_manager: manager,
            telemetry: test_telemetry(),
        };

        let (mut session, mut client) =
            session_for_request(b"GET /missing HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
        let mut ctx = proxy.new_ctx();
        let should_respond = proxy
            .request_filter(&mut session, &mut ctx)
            .await
            .expect("request filter");
        assert!(should_respond);
        let mut buf = [0u8; 512];
        let read = tokio::time::timeout(std::time::Duration::from_secs(1), client.read(&mut buf))
            .await
            .expect("read timeout")
            .expect("read response");
        let response = String::from_utf8_lossy(&buf[..read]);
        assert!(response.contains(" 404 "), "response was {response:?}");
    }

    #[tokio::test]
    async fn request_filter_skips_selection_when_total_weight_zero() {
        let routes = vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Exact,
                path: "/api".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                destinations: vec![
                    WeightedDestination {
                        upstream: "blue".to_string(),
                        weight: 0,
                    },
                    WeightedDestination {
                        upstream: "green".to_string(),
                        weight: 0,
                    },
                ],
                compiled_regex: None,
            }],
        }];
        let router = Arc::new(Router::new(routes).expect("routes"));
        let manager = Manager::new(&[upstream("blue", 8081), upstream("green", 8082)]);
        let proxy = Proxy {
            router,
            upstream_manager: manager,
            telemetry: test_telemetry(),
        };

        let (mut session, _client) =
            session_for_request(b"GET /api HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
        let mut ctx = proxy.new_ctx();
        let should_respond = proxy
            .request_filter(&mut session, &mut ctx)
            .await
            .expect("request filter");
        assert!(!should_respond);
        assert!(ctx.upstream_name.is_none());
    }
}

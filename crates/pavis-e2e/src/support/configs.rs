use pavis_core::{
    AccessLogPolicy, ConnectTimeout, ConnectionLimit, Destination, Discovery, Duration, Endpoint,
    EndpointAddr, Host, HttpVersion, IdleTimeout, Listener, ListenerName, LoadBalancer, Metrics,
    Path, PathMatch, Pool, RetryPolicy, Rewrite, RewriteHost, RewritePath, RouteAction,
    RuntimeConfig, ServiceName, Telemetry, Timeout, TracingPolicy, Upstream, UpstreamId,
    UpstreamName, Weight, WorkerCount,
};
use std::net::SocketAddr;
use std::num::{NonZeroU16, NonZeroU32};

pub fn runtime_config(
    listen_addr: SocketAddr,
    upstream_a: (&str, SocketAddr),
    upstream_b: (&str, SocketAddr),
    route_upstream: &str,
) -> RuntimeConfig {
    RuntimeConfig {
        listeners: vec![Listener {
            name: ListenerName("default".to_string()),
            address: listen_addr,
            workers: WorkerCount::Auto,
            tls: pavis_core::TlsConfig::Disabled,
        }],
        telemetry: Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName("pavis-integrated".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Stdout,
            tracing: TracingPolicy::Disabled,
        },
        upstreams: vec![
            upstream(1, upstream_a.0, upstream_a.1),
            upstream(2, upstream_b.0, upstream_b.1),
        ],
        routes: vec![pavis_core::VirtualHost {
            host: Host("*".to_string()),
            paths: vec![pavis_core::Route {
                matcher: PathMatch::Prefix {
                    path: Path("/".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled,
                response_headers: pavis_core::HeadersPolicy::Disabled,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName(route_upstream.to_string()),
                    weight: Weight(NonZeroU16::new(1).expect("nonzero weight")),
                }]),
            }],
        }],
    }
}

pub fn upstream(id: u16, name: &str, addr: SocketAddr) -> Upstream {
    Upstream {
        id: UpstreamId(NonZeroU16::new(id).expect("nonzero upstream id")),
        name: UpstreamName(name.to_string()),
        discovery: Discovery::Static,
        balancer: LoadBalancer::RoundRobin,
        protocol: HttpVersion::H1,
        pool: Pool {
            idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).expect("nonzero timeout"))),
            connect: ConnectTimeout::Enabled(Duration(
                NonZeroU32::new(5_000).expect("nonzero timeout"),
            )),
            max: ConnectionLimit::Unlimited,
        },
        tls: pavis_core::TlsPolicy::Disabled,
        endpoints: vec![Endpoint {
            address: EndpointAddr::Ip {
                address: addr.ip(),
                port: pavis_core::Port(NonZeroU16::new(addr.port()).expect("nonzero port")),
            },
            weight: Weight(NonZeroU16::new(1).expect("nonzero weight")),
        }],
    }
}

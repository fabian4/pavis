mod common;

use common::base_config;
use pavis::router::Router;
use pavis_core::{
    ConnectTimeout, ConnectionLimit, Destination, Duration, Endpoint, EndpointAddr, Host,
    HttpVersion, IdleTimeout, LoadBalancer, Path, PathMatch, Pool, RetryPolicy, Rewrite,
    RewriteHost, RewritePath, RouteAction, Timeout, Upstream, UpstreamBuilder, UpstreamId,
    UpstreamName, VirtualHost, Weight,
};
use std::net::{IpAddr, Ipv4Addr};
use std::num::{NonZeroU16, NonZeroU32};

fn upstream(name: &str, id: u16, port: u16) -> Upstream {
    UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(id).unwrap()))
        .name(UpstreamName(name.to_string()))
        .discovery(pavis_core::Discovery::Static)
        .balancer(LoadBalancer::Random)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit::Unlimited,
        })
        .tls(pavis_core::TlsPolicy::Disabled)
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port: pavis_core::Port(NonZeroU16::new(port).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
        .build()
        .expect("upstream")
}

#[test]
fn test_routing_vhost_precedence() {
    let mut config = base_config();
    config.upstreams.push(upstream("api-upstream", 1, 8084));
    config.upstreams.push(upstream("web-upstream", 2, 8085));
    config
        .upstreams
        .push(upstream("wildcard-upstream", 3, 8086));
    config.routes = vec![
        VirtualHost {
            host: Host("*".to_string()),
            paths: vec![pavis_core::Route {
                matcher: PathMatch::Exact {
                    path: Path("/".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("wildcard-upstream".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            }],
        },
        VirtualHost {
            host: Host("api.com".to_string()),
            paths: vec![pavis_core::Route {
                matcher: PathMatch::Exact {
                    path: Path("/".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("api-upstream".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            }],
        },
        VirtualHost {
            host: Host("web.com".to_string()),
            paths: vec![pavis_core::Route {
                matcher: PathMatch::Exact {
                    path: Path("/".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("web-upstream".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            }],
        },
    ];

    let router = Router::new(config.routes).expect("Failed to create router");

    let (vhost, _route) = router
        .match_request(Some("api.com"), "/")
        .expect("api.com should match");
    assert_eq!(vhost.host.0, "api.com");

    let (vhost, _route) = router
        .match_request(Some("web.com"), "/")
        .expect("web.com should match");
    assert_eq!(vhost.host.0, "web.com");

    let (vhost, _route) = router
        .match_request(Some("unknown.com"), "/")
        .expect("wildcard should match");
    assert_eq!(vhost.host.0, "*");
}

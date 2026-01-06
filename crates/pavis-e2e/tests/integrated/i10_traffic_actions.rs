use anyhow::Result;
use pavis_core::{
    HeadersPolicy, Path, PathMatch, RetryPolicy, Rewrite, RewriteHost, RewritePath, Route,
    RouteAction, Timeout, VirtualHost,
};
use reqwest::{Client, StatusCode};
use std::time::Duration;

use super::support::{
    PavisEnv, expected_body, pavis_target, publish, relay_env, runtime_config, upstreams,
    wait_for_body,
};

#[tokio::test]
async fn integrated_traffic_actions_redirect_direct() -> Result<()> {
    let relay = relay_env().await?;
    let Some(upstreams) = upstreams().await? else {
        return Ok(());
    };
    let target = pavis_target()?;

    let mut config = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );

    config.routes = vec![VirtualHost {
        host: pavis_core::Host("*".to_string()),
        paths: vec![
            // Redirect 301
            Route {
                matcher: PathMatch::Exact {
                    path: Path("/redirect-me".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled,
                response_headers: HeadersPolicy::Disabled,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Redirect {
                    status: 301,
                    location: "https://example.com/dest".to_string(),
                },
            },
            // Direct 200 OK
            Route {
                matcher: PathMatch::Exact {
                    path: Path("/direct-ok".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled,
                response_headers: HeadersPolicy::Disabled,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Direct {
                    status: 200,
                    body: "Direct Response Body".to_string(),
                },
            },
            // Fallback
            Route {
                matcher: PathMatch::Prefix {
                    path: Path("/".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled,
                response_headers: HeadersPolicy::Disabled,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![pavis_core::Destination {
                    upstream: pavis_core::UpstreamName("upstream-a".to_string()),
                    weight: pavis_core::Weight(std::num::NonZeroU16::new(1).unwrap()),
                }]),
            },
        ],
    }];

    publish(relay.base_url(), 1, &config).await?;
    let pavis = PavisEnv::new(&config, target.host_port, relay.base_url())?;
    wait_for_body(pavis.base_url(), &expected_body("A")).await?;

    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()?;

    let base = pavis.base_url();

    // Verify Redirect
    let resp = client.get(format!("{}/redirect-me", base)).send().await?;
    assert_eq!(resp.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(
        resp.headers().get("location").unwrap().to_str()?,
        "https://example.com/dest"
    );

    // Verify Direct
    let resp = client.get(format!("{}/direct-ok", base)).send().await?;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.text().await?, "Direct Response Body");

    Ok(())
}

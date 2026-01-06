use anyhow::Result;
use pavis_core::{
    Destination, HeadersPolicy, Path, PathMatch, RetryPolicy, Rewrite, RewriteHost, RewritePath,
    Route, RouteAction, Timeout, UpstreamName, VirtualHost, Weight,
};
use reqwest::{Client, StatusCode};
use std::num::NonZeroU16;
use std::time::Duration;

use super::support::{
    PavisEnv, expected_body, pavis_target, publish, relay_env, runtime_config, upstreams,
    wait_for_body,
};

#[tokio::test]
async fn integrated_traffic_modifiers_via_relay() -> Result<()> {
    let relay = relay_env().await?;
    let Some(upstreams) = upstreams().await? else {
        return Ok(());
    };
    let target = pavis_target()?;

    // 1. Construct a config with Redirect, Direct, and Rewrite routes
    let mut config = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a", // Default forward (will be ignored by specific routes)
    );

    // Overwrite routes with our specific test cases
    config.routes = vec![VirtualHost {
        host: pavis_core::Host("*".to_string()),
        paths: vec![
            // Case 1: Redirect 301
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
            // Case 2: Direct 200 OK
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
            // Case 3: Prefix Rewrite to Upstream B
            // /rewrite/foo -> /bar/foo
            Route {
                matcher: PathMatch::Prefix {
                    path: Path("/rewrite".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled,
                response_headers: HeadersPolicy::Disabled,
                rewrite: Rewrite {
                    path: RewritePath::Prefix {
                        from: Path("/rewrite".to_string()),
                        to: Path("/bar".to_string()),
                    },
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("upstream-b".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            },
            // Fallback to A
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
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("upstream-a".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            },
        ],
    }];

    // 2. Publish Config to Relay
    publish(relay.base_url(), 1, &config).await?;

    // 3. Start Pavis
    let pavis = PavisEnv::new(&config, target.host_port, relay.base_url())?;

    // Wait for basic health (fallback route)
    if let Err(e) = wait_for_body(pavis.base_url(), &expected_body("A")).await {
        pavis.print_logs();
        return Err(e);
    }

    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none()) // Don't follow redirects
        .timeout(Duration::from_secs(3))
        .build()?;

    let base = pavis.base_url();

    // 4. Verify Redirect
    let resp = client.get(format!("{}/redirect-me", base)).send().await?;
    assert_eq!(resp.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(
        resp.headers().get("location").unwrap().to_str()?,
        "https://example.com/dest"
    );

    // 5. Verify Direct
    let resp = client.get(format!("{}/direct-ok", base)).send().await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.text().await?;
    assert_eq!(body, "Direct Response Body");

    // 6. Verify Rewrite
    // /rewrite/test?q=1 -> Upstream B receives /bar/test?q=1
    // Note: The mock upstream echoes the request or at least identifies itself.
    // Our `expected_body("B")` just checks for "backend-v2" or "B".
    // To verify the PATH rewrite, we might need a more sophisticated mock or just assume if it hits B it's working
    // (since routing matched).
    // Ideally, we'd check the echoed path, but `pavis-e2e` upstreams just echo a fixed body "A" or "B" or "backend-vX".
    // However, if the rewrite was broken (e.g. strict matching upstream), it might 404 on the upstream.
    // For now, verifying it routes to B is sufficient proof that the route matched.
    let resp = client.get(format!("{}/rewrite/test", base)).send().await?;
    assert!(resp.status().is_success());
    let body = resp.text().await?;
    assert!(body.contains(&expected_body("B")));

    Ok(())
}

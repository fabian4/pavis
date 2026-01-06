use anyhow::Result;
use pavis_core::{
    Destination, HeadersPolicy, Path, PathMatch, RetryPolicy, Rewrite, RewriteHost, RewritePath,
    Route, RouteAction, Timeout, UpstreamName, VirtualHost, Weight,
};
use std::num::NonZeroU16;

use super::support::{
    PavisEnv, expected_body, pavis_target, publish, relay_env, runtime_config, upstreams,
    wait_for_body,
};

#[tokio::test]
async fn integrated_rewrite_propagation() -> Result<()> {
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
            // Prefix Rewrite to Upstream B
            Route {
                matcher: PathMatch::Prefix {
                    path: Path("/rewrite".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled,
                response_headers: HeadersPolicy::Disabled,
                principal: pavis_core::Principal::Any,
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
                principal: pavis_core::Principal::Any,
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

    publish(relay.base_url(), 1, &config).await?;
    let pavis = PavisEnv::new(&config, target.host_port, relay.base_url())?;
    wait_for_body(pavis.base_url(), &expected_body("A")).await?;

    let client = reqwest::Client::new();
    let base = pavis.base_url();

    // Verify Rewrite
    let resp = client.get(format!("{}/rewrite/test", base)).send().await?;
    assert!(resp.status().is_success());
    let body = resp.text().await?;
    assert!(body.contains(&expected_body("B")));

    Ok(())
}

use crate::common;

use anyhow::Result;
use pavis_e2e::support::PavisConfigScenario;
use pavis_e2e::support::get_upstream_name;

/// Test that HTTP version configuration works correctly.
/// Note: The echo-server backends are HTTP/1.1 only, so we can't verify actual H2 negotiation.
/// This test verifies that the config is accepted and routing works with different http_version settings.
#[tokio::test]
async fn test_http_version_config() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::HttpVersion).await;

    // 1. Route to H1 upstream should work
    let upstream = get_upstream_name(&client, "/h1/test").await?;
    assert_eq!(upstream, "backend-v1", "H1 upstream route should work");

    // 2. Route to H2 upstream should work (falls back to H1 with echo-server)
    let upstream = get_upstream_name(&client, "/h2/test").await?;
    assert_eq!(
        upstream, "backend-v2",
        "H2 upstream route should work (fallback to H1)"
    );

    Ok(())
}

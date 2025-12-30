use crate::common;

use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::get_upstream_name;

#[tokio::test]
async fn test_route_matching() -> Result<()> {
    let (client, _env) = common::setup(PavisScenario::RouteMatching).await;

    // 1. Exact Match: Should go to v1
    let upstream = get_upstream_name(&client, "/exact-only").await?;
    assert_eq!(upstream, "backend-v1", "Exact match failed for /exact-only");

    // 2. Exact Match miss: /exact-only/something should NOT match 'exact' /exact-only.
    // It should match the '/' prefix route instead, which also goes to v1 in our config.
    let upstream = get_upstream_name(&client, "/exact-only/something").await?;
    assert_eq!(
        upstream, "backend-v1",
        "Fallback to prefix / failed for /exact-only/something"
    );

    // 3. Prefix Match: Should go to v2
    let upstream = get_upstream_name(&client, "/prefix-match").await?;
    assert_eq!(
        upstream, "backend-v2",
        "Prefix match failed for /prefix-match"
    );

    // 4. Prefix Match subpath: Should go to v2
    let upstream = get_upstream_name(&client, "/prefix-match/anything").await?;
    assert_eq!(
        upstream, "backend-v2",
        "Prefix match subpath failed for /prefix-match/anything"
    );

    Ok(())
}

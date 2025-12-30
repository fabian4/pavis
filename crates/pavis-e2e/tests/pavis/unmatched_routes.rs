use crate::common;

use anyhow::Result;
use pavis_e2e::support::PavisScenario;

const BASE_URL: &str = "http://localhost:8080";

#[tokio::test]
async fn test_unmatched_routes_return_404() -> Result<()> {
    let (client, _env) = common::setup(PavisScenario::UnmatchedRoutes).await;

    // 1. Request to unmatched path should return 404
    let resp = client
        .get(format!("{}/nonexistent", BASE_URL))
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        404,
        "Unmatched path should return 404"
    );

    // 2. Request to wrong host should return 404 (config only has example.com)
    let resp = client
        .get(format!("{}/api/test", BASE_URL))
        .header("Host", "wrong-host.com")
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 404, "Wrong host should return 404");

    // 3. Request to correct host and path should succeed
    let resp = client
        .get(format!("{}/api/test", BASE_URL))
        .header("Host", "example.com")
        .send()
        .await?;
    assert!(
        resp.status().is_success(),
        "Correct host and path should succeed"
    );

    Ok(())
}

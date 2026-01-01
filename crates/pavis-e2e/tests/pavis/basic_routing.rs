use crate::common;

use anyhow::Result;
use pavis_e2e::support::PavisConfigScenario;
use pavis_e2e::support::get_upstream_name;

#[tokio::test]
async fn test_basic_routing() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::BasicRouting).await;

    for _ in 1..=10 {
        let upstream = get_upstream_name(&client, "/").await?;
        assert!(
            upstream.contains("backend-v"),
            "Upstream should be a backend"
        );
    }
    Ok(())
}

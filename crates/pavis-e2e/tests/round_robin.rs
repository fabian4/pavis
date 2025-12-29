mod common;

use anyhow::Result;
use pavis_e2e::support::get_upstream_name;

#[tokio::test]
async fn test_round_robin() -> Result<()> {
    let (client, _env) = common::setup("round_robin.yaml").await;

    let mut prev_upstream = String::new();

    for i in 1..=6 {
        let upstream = get_upstream_name(&client, "/round-robin").await?;
        if i > 1 {
            assert_ne!(
                upstream, prev_upstream,
                "Upstream should change in round-robin (request {})",
                i
            );
        }
        prev_upstream = upstream;
    }
    Ok(())
}

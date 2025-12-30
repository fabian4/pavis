use crate::common;

use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::get_upstream_name;

#[tokio::test]
async fn test_weighted_splitting() -> Result<()> {
    let (client, _env) = common::setup(PavisScenario::WeightedSplitting).await;

    let mut v1_count = 0;
    let mut v2_count = 0;
    let total_requests = 50;

    for _ in 0..total_requests {
        let upstream = get_upstream_name(&client, "/weighted-test").await?;
        if upstream.contains("backend-v1") {
            v1_count += 1;
        } else if upstream.contains("backend-v2") {
            v2_count += 1;
        }
    }

    println!(
        "Weighted Result: V1={} V2={} (Total: {})",
        v1_count, v2_count, total_requests
    );

    // V1 (80%) should be significantly higher than V2 (20%)
    assert!(
        v1_count > v2_count,
        "V1 ({}) should be more than V2 ({})",
        v1_count,
        v2_count
    );
    assert!(
        v1_count >= 30,
        "V1 ({}) should have at least 60% of traffic (expected 80%)",
        v1_count
    );

    Ok(())
}

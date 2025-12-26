mod common;

use anyhow::Result;
use pavis_e2e::utils::get_upstream_name;

#[tokio::test]
async fn test_upstream_weight() -> Result<()> {
    let (client, _env) = common::setup("upstream_weight.yaml").await;

    let mut v1_count = 0;
    let mut v2_count = 0;
    let total_requests = 40;

    for _ in 0..total_requests {
        let upstream = get_upstream_name(&client, "/").await?;
        if upstream.contains("backend-v1") {
            v1_count += 1;
        } else if upstream.contains("backend-v2") {
            v2_count += 1;
        }
    }

    println!(
        "Upstream Weight Result: V1={} V2={} (Total: {})",
        v1_count, v2_count, total_requests
    );

    // Expected: 3:1 ratio. 30 for v1, 10 for v2.
    // Allow small variance if implementation changes, but with deterministic RR it should be close.
    assert!(
        v1_count > v2_count,
        "V1 ({}) should be significantly more than V2 ({})",
        v1_count,
        v2_count
    );
    assert!(v1_count >= 25, "V1 ({}) should be ~30", v1_count);
    assert!(v2_count >= 5, "V2 ({}) should be ~10", v2_count);

    Ok(())
}

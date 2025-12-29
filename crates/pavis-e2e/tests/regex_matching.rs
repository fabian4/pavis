mod common;

use anyhow::Result;
use pavis_e2e::support::get_upstream_name;

#[tokio::test]
async fn test_regex_matching() -> Result<()> {
    let (client, _env) = common::setup("regex_matching.yaml").await;

    // 1. Regex Match: /api/v1/users/123 should go to backend-v1
    let upstream = get_upstream_name(&client, "/api/v1/users/123").await?;
    assert_eq!(
        upstream, "backend-v1",
        "Regex match failed for /api/v1/users/123"
    );

    // 2. Regex Match: /api/v2/users/456 should also match
    let upstream = get_upstream_name(&client, "/api/v2/users/456").await?;
    assert_eq!(
        upstream, "backend-v1",
        "Regex match failed for /api/v2/users/456"
    );

    // 3. Regex NO Match: /api/v1/users/abc (non-numeric id) should fallback to v1
    let upstream = get_upstream_name(&client, "/api/v1/users/abc").await?;
    assert_eq!(
        upstream, "backend-v1",
        "Non-numeric user id should fallback to v1"
    );

    // 4. Regex Match: /posts/hello-world should go to backend-v2
    let upstream = get_upstream_name(&client, "/posts/hello-world").await?;
    assert_eq!(
        upstream, "backend-v2",
        "Regex match failed for /posts/hello-world"
    );

    // 5. Regex Match: /posts/my-first-post-2024 should match v2
    let upstream = get_upstream_name(&client, "/posts/my-first-post-2024").await?;
    assert_eq!(
        upstream, "backend-v2",
        "Regex match failed for /posts/my-first-post-2024"
    );

    // 6. Regex NO Match: /posts/Hello_World (uppercase, underscore) should fallback to v1
    let upstream = get_upstream_name(&client, "/posts/Hello_World").await?;
    assert_eq!(upstream, "backend-v1", "Invalid slug should fallback to v1");

    Ok(())
}

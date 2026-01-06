mod common;

use anyhow::Result;
use pavis_e2e::support::PavisConfigScenario;

#[tokio::test]
async fn test_path_rewrite_preserves_simple_query() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::PathRewriteQuery).await;

    // Send request with simple query string
    let resp = client
        .get("http://localhost:8080/api/v1/users?id=123")
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    // The backend should receive the rewritten path with query preserved
    let body = resp.text().await?;
    assert!(
        body.contains("/v2/users"),
        "Expected rewritten path in response"
    );
    assert!(
        body.contains("id=123"),
        "Expected query parameter preserved"
    );

    Ok(())
}

#[tokio::test]
async fn test_path_rewrite_preserves_complex_query() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::PathRewriteQuery).await;

    // Send request with multiple query parameters
    let resp = client
        .get("http://localhost:8080/api/v1/search?q=hello&page=2&limit=10&sort=name")
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let body = resp.text().await?;
    assert!(body.contains("/v2/search"), "Expected rewritten path");
    assert!(
        body.contains("q=hello"),
        "Expected query parameter q preserved"
    );
    assert!(
        body.contains("page=2"),
        "Expected query parameter page preserved"
    );
    assert!(
        body.contains("limit=10"),
        "Expected query parameter limit preserved"
    );
    assert!(
        body.contains("sort=name"),
        "Expected query parameter sort preserved"
    );

    Ok(())
}

#[tokio::test]
async fn test_path_rewrite_preserves_encoded_query() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::PathRewriteQuery).await;

    // Send request with URL-encoded query parameters
    let resp = client
        .get("http://localhost:8080/api/v1/search?q=hello%20world&filter=name%3Dtest")
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let body = resp.text().await?;
    assert!(body.contains("/v2/search"), "Expected rewritten path");
    // The query parameters should be preserved (encoding may vary)
    assert!(
        body.contains("hello") && body.contains("world"),
        "Expected encoded query preserved"
    );

    Ok(())
}

#[tokio::test]
async fn test_path_rewrite_without_query() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::PathRewriteQuery).await;

    // Send request without query string
    let resp = client
        .get("http://localhost:8080/api/v1/users")
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let body = resp.text().await?;
    assert!(body.contains("/v2/users"), "Expected rewritten path");

    Ok(())
}

#[tokio::test]
async fn test_path_rewrite_with_empty_query() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::PathRewriteQuery).await;

    // Send request with empty query string
    let resp = client
        .get("http://localhost:8080/api/v1/users?")
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let body = resp.text().await?;
    assert!(body.contains("/v2/users"), "Expected rewritten path");

    Ok(())
}

#[tokio::test]
async fn test_path_rewrite_nested_paths_with_query() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::PathRewriteQuery).await;

    // Test rewrite with deeply nested paths
    let resp = client
        .get("http://localhost:8080/api/v1/users/123/orders/456?include=items&expand=true")
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let body = resp.text().await?;
    assert!(
        body.contains("/v2/users/123/orders/456"),
        "Expected rewritten nested path"
    );
    assert!(body.contains("include=items"), "Expected query preserved");
    assert!(body.contains("expand=true"), "Expected query preserved");

    Ok(())
}

#[tokio::test]
async fn test_exact_match_rewrite_with_query() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::PathRewriteQuery).await;

    // Test exact match path rewrite with query
    let resp = client
        .get("http://localhost:8080/old-path?redirect=true")
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let body = resp.text().await?;
    assert!(body.contains("/new-path"), "Expected exact match rewrite");
    assert!(body.contains("redirect=true"), "Expected query preserved");

    Ok(())
}

#[tokio::test]
async fn test_prefix_rewrite_multiple_requests() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::PathRewriteQuery).await;

    // Test multiple requests to ensure query preservation is consistent
    for i in 0..10 {
        let resp = client
            .get(&format!(
                "http://localhost:8080/api/v1/test?iteration={}&value={}",
                i,
                i * 2
            ))
            .send()
            .await?;

        assert_eq!(resp.status(), 200);

        let body = resp.text().await?;
        assert!(body.contains("/v2/test"), "Expected rewritten path");
        assert!(
            body.contains(&format!("iteration={}", i)),
            "Expected iteration parameter preserved"
        );
        assert!(
            body.contains(&format!("value={}", i * 2)),
            "Expected value parameter preserved"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_host_rewrite_preserves_query() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::PathRewriteQuery).await;

    // Test that host rewrite also preserves query parameters
    let resp = client
        .get("http://localhost:8080/rewrite-host/resource?key=value&foo=bar")
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let body = resp.text().await?;
    assert!(
        body.contains("key=value"),
        "Expected query preserved with host rewrite"
    );
    assert!(
        body.contains("foo=bar"),
        "Expected query preserved with host rewrite"
    );

    Ok(())
}

#[tokio::test]
async fn test_query_with_special_characters() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::PathRewriteQuery).await;

    // Test query parameters with special characters
    let resp = client
        .get("http://localhost:8080/api/v1/data?filter=status:active&tags=rust,http,proxy")
        .send()
        .await?;

    assert_eq!(resp.status(), 200);

    let body = resp.text().await?;
    assert!(body.contains("/v2/data"), "Expected rewritten path");
    assert!(
        body.contains("filter") && body.contains("active"),
        "Expected filter parameter preserved"
    );
    assert!(
        body.contains("tags") && body.contains("rust"),
        "Expected tags parameter preserved"
    );

    Ok(())
}

#[tokio::test]
async fn test_weighted_routing_preserves_query() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::PathRewriteQuery).await;

    // Test that weighted routing also preserves query with path rewrite
    for _ in 0..20 {
        let resp = client
            .get("http://localhost:8080/api/v1/weighted?test=value")
            .send()
            .await?;

        assert_eq!(resp.status(), 200);

        let body = resp.text().await?;
        assert!(body.contains("/v2/weighted"), "Expected rewritten path");
        assert!(body.contains("test=value"), "Expected query preserved");
    }

    Ok(())
}

mod common;

use anyhow::Result;
use pavis_e2e::utils::get_response_json;
use std::collections::HashMap;

async fn get_upstream_with_host(
    client: &reqwest::Client,
    path: &str,
    host: &str,
) -> Result<String> {
    let mut headers = HashMap::new();
    headers.insert("Host".to_string(), host.to_string());

    let json = get_response_json(client, path, headers).await?;

    // Extract SERVICE_NAME from echo-server response
    if let Some(name) = json
        .get("os")
        .and_then(|os| os.get("env"))
        .and_then(|env| env.get("SERVICE_NAME"))
        .and_then(|v| v.as_str())
    {
        return Ok(name.to_string());
    }

    // Fallback
    let text = json.to_string();
    if text.contains("backend-v1") {
        return Ok("backend-v1".to_string());
    }
    if text.contains("backend-v2") {
        return Ok("backend-v2".to_string());
    }

    Err(anyhow::anyhow!("Could not identify upstream"))
}

#[tokio::test]
async fn test_wildcard_host_matching() -> Result<()> {
    let (client, _env) = common::setup("wildcard_host.yaml").await;

    // 1. Specific host "api.example.com" should go to backend-v1
    let upstream = get_upstream_with_host(&client, "/test", "api.example.com").await?;
    assert_eq!(
        upstream, "backend-v1",
        "Specific host api.example.com should route to backend-v1"
    );

    // 2. Wildcard should catch "other.example.com" -> backend-v2
    let upstream = get_upstream_with_host(&client, "/test", "other.example.com").await?;
    assert_eq!(
        upstream, "backend-v2",
        "Wildcard should catch other.example.com and route to backend-v2"
    );

    // 3. Wildcard should catch "random-host.io" -> backend-v2
    let upstream = get_upstream_with_host(&client, "/test", "random-host.io").await?;
    assert_eq!(
        upstream, "backend-v2",
        "Wildcard should catch random-host.io and route to backend-v2"
    );

    // 4. No host header should match wildcard -> backend-v2
    let upstream = get_upstream_with_host(&client, "/test", "localhost").await?;
    assert_eq!(
        upstream, "backend-v2",
        "localhost should match wildcard and route to backend-v2"
    );

    Ok(())
}

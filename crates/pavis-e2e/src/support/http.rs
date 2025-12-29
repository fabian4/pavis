use anyhow::{Context, Result};
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

/// Base URL for the proxy.
pub const BASE_URL: &str = "http://localhost:8080";

/// Waits for the Pavis proxy to be ready.
///
/// # Errors
/// Returns an error if the proxy does not respond within the timeout.
pub async fn wait_for_pavis(client: &Client) -> Result<()> {
    println!("🚀 Starting E2E Tests...");
    println!("Waiting for Pavis to be ready at {BASE_URL}...");

    for _ in 0..30 {
        if client.get(BASE_URL).send().await.is_ok() {
            println!("✅ Pavis is up!");
            return Ok(());
        }
        print!(".");
        sleep(Duration::from_secs(1)).await;
    }

    Err(anyhow::anyhow!("❌ Timeout waiting for Pavis to start."))
}

/// Helper to get the upstream service name from a proxy response.
///
/// # Errors
/// Returns an error if the request fails or the response cannot be parsed.
pub async fn get_upstream_name(client: &Client, path: &str) -> Result<String> {
    let url = format!("{BASE_URL}{path}");
    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to send request")?;

    let status = resp.status();
    let text = resp.text().await.context("Failed to read response body")?;

    if !status.is_success() {
        return Err(anyhow::anyhow!(
            "Request failed with status {status}: {text}"
        ));
    }

    // Try to parse as generic JSON first
    if let Some(name) = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|json| {
            json.get("os")
                .and_then(|os| os.get("env"))
                .and_then(|env| env.get("SERVICE_NAME"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
        })
    {
        return Ok(name);
    }

    // Fallback: Use string search if JSON structure varies
    if text.contains("backend-v1") {
        return Ok("backend-v1".to_string());
    }
    if text.contains("backend-v2") {
        return Ok("backend-v2".to_string());
    }

    Err(anyhow::anyhow!("Could not identify upstream from response"))
}

/// Helper to get a JSON response from the proxy.
///
/// # Errors
/// Returns an error if the request fails or JSON parsing fails.
pub async fn get_response_json<S: ::std::hash::BuildHasher>(
    client: &Client,
    path: &str,
    headers: HashMap<String, String, S>,
) -> Result<serde_json::Value> {
    let url = format!("{BASE_URL}{path}");
    let mut req = client.get(&url);
    for (k, v) in headers {
        req = req.header(k, v);
    }

    let resp = req.send().await.context("Failed to send request")?;
    let text = resp.text().await.context("Failed to read response body")?;

    serde_json::from_str(&text).context("Failed to parse JSON")
}

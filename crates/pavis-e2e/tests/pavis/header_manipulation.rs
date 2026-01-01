use crate::common;

use anyhow::Result;
use pavis_e2e::support::PavisConfigScenario;
use pavis_e2e::support::get_response_json;
use std::collections::HashMap;

#[tokio::test]
async fn test_header_manipulation() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::HeaderManipulation).await;

    let mut send_headers = HashMap::new();
    send_headers.insert(
        "X-Pavis-Remove-Me".to_string(),
        "ShouldNotBeSeen".to_string(),
    );
    send_headers.insert("X-Keep-Me".to_string(), "StillHere".to_string());

    let json = get_response_json(&client, "/headers", send_headers).await?;

    // Echo server puts headers in the "headers" key or "request.headers"
    let received_headers = json
        .get("headers")
        .or_else(|| json.get("request").and_then(|r| r.get("headers")))
        .ok_or_else(|| anyhow::anyhow!("Missing headers in response"))?;

    // Helper to get string value from json
    let get_header = |name: &str| received_headers.get(name).and_then(|v| v.as_str());

    // 1. Verify Addition
    assert_eq!(get_header("x-pavis-added"), Some("Verified"));
    assert_eq!(get_header("x-multi-word"), Some("Hello World"));
    assert_eq!(get_header("x-proxy-by"), Some("Pavis"));

    // 2. Verify Removal
    assert!(
        get_header("x-pavis-remove-me").is_none(),
        "Header should have been removed"
    );

    // 3. Verify Preservation
    assert_eq!(get_header("x-keep-me"), Some("StillHere"));

    Ok(())
}

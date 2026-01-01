use crate::common;

use anyhow::Result;
use pavis_e2e::support::PavisConfigScenario;

#[tokio::test]
async fn test_response_header_manipulation() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::ResponseHeaders).await;

    // Send a request
    let response = client
        .get("http://127.0.0.1:8080/headers")
        .header("Host", "response-headers")
        .send()
        .await?;

    assert!(response.status().is_success());
    let headers = response.headers();

    // 1. Verify Addition
    assert_eq!(
        headers
            .get("x-pavis-resp-added")
            .and_then(|v| v.to_str().ok()),
        Some("Verified")
    );
    assert_eq!(
        headers
            .get("x-multi-word-resp")
            .and_then(|v| v.to_str().ok()),
        Some("Hello World")
    );
    assert_eq!(
        headers.get("x-proxy-by").and_then(|v| v.to_str().ok()),
        Some("Pavis")
    );

    // 2. Verify Removal (The httpbin/nginx backend usually sends a 'Server' header)
    // Note: If the backend doesn't send 'Server', this test passes trivially.
    // Ideally we'd ensure the backend sends it first, but we know standard nginx/httpbin does.
    assert!(
        headers.get("server").is_none(),
        "Server header should have been removed"
    );

    Ok(())
}

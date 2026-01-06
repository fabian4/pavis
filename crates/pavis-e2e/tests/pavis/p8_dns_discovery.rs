mod common;

use anyhow::Result;
use pavis_e2e::support::PavisConfigScenario;

#[tokio::test]
async fn test_dns_discovery_connectivity() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::DnsDiscovery).await;

    // Send a request. It should be routed to 'backend-dns' which resolves 'localhost' (or backend-v1)
    // and forwards to port 8081.
    let resp = client.get("http://localhost:8080/").send().await?;

    assert_eq!(resp.status(), 200);
    let body = resp.text().await?;
    // backend-v1 is an echo server, usually responds with JSON info
    assert!(
        body.contains("backend-v1") || body.contains("SERVICE_NAME"),
        "Response should be from backend"
    );

    Ok(())
}

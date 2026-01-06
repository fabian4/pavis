use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::expected_body;
use pavis_e2e::support::relay::RelayOptions;
use pavis_e2e::support::runtime_config;
use pavis_e2e::support::to_yaml;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn chaos_reloads_converge_to_latest() -> Result<()> {
    let mut options = RelayOptions::default();
    options.enable_file_ingest = true;
    options.ingest_debounce_ms = 100;

    let scenario = PavisScenario::new(options, true, true).await?;
    let pavis = scenario.pavis.as_ref().expect("pavis");
    let upstreams = scenario.upstreams.as_ref().expect("upstreams");

    scenario.expect_body(&expected_body("A")).await?;

    let listen_addr = pavis
        .base_url()
        .trim_start_matches("http://")
        .parse()
        .expect("listen addr");
    let ingest_path = scenario.relay.ingest_path.as_ref().expect("ingest path");
    let start_version = scenario.relay.client().status().await?.version;

    let updates = ["B", "A", "B", "A", "B"];
    for label in updates {
        let route_upstream = if label == "A" {
            "upstream-a"
        } else {
            "upstream-b"
        };
        let config = runtime_config(
            listen_addr,
            ("upstream-a", upstreams.a),
            ("upstream-b", upstreams.b),
            route_upstream,
        );
        let yaml = to_yaml(&config);
        std::fs::write(ingest_path, yaml)?;
        sleep(Duration::from_millis(50)).await;
    }

    scenario.wait_for_relay_version(start_version + 1).await?;
    scenario.expect_body(&expected_body("B")).await?;

    Ok(())
}

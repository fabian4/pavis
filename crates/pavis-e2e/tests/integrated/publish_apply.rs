use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::expected_body;
use pavis_e2e::support::relay::RelayOptions;
use pavis_e2e::support::runtime_config;

#[tokio::test]
async fn integrated_publish_and_apply_updates() -> Result<()> {
    let mut options = RelayOptions::default();
    options.enable_file_ingest = true;
    options.ingest_debounce_ms = 500;

    let scenario = PavisScenario::new(options, true, true).await?;
    let pavis = scenario.pavis.as_ref().unwrap();
    let upstreams = scenario.upstreams.as_ref().unwrap();

    // Initial state (A)
    scenario.expect_body(&expected_body("A")).await?;

    // Update to B
    let target_addr = pavis
        .base_url()
        .trim_start_matches("http://")
        .parse()
        .expect("valid addr");

    let config_v2 = runtime_config(
        target_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-b",
    );

    scenario.apply_config(&config_v2).await?;

    scenario.expect_body(&expected_body("B")).await?;

    Ok(())
}

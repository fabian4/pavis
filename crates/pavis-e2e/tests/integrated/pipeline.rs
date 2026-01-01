use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::expected_body;
use pavis_e2e::support::relay::RelayOptions;
use pavis_e2e::support::runtime_config;

#[tokio::test]
async fn integrated_file_ingest_pipeline() -> Result<()> {
    let mut options = RelayOptions::default();
    options.enable_file_ingest = true;
    options.ingest_debounce_ms = 500;

    // Start everything (Relay + Pavis + Upstreams + Initial Config)
    let scenario = PavisScenario::new(options, true).await?;

    let pavis = scenario.pavis.as_ref().unwrap();
    let upstreams = &scenario.upstreams;

    // Initial verification (A)
    scenario.expect_body(&expected_body("A")).await?;

    // Switch to B
    let target_addr = pavis
        .base_url()
        .trim_start_matches("http://")
        .parse()
        .expect("valid addr");

    let config_b = runtime_config(
        target_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-b",
    );
    scenario.apply_config(&config_b).await?;

    scenario.expect_body(&expected_body("B")).await?;

    // Switch back to A
    let config_a = runtime_config(
        target_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );
    scenario.apply_config(&config_a).await?;

    scenario.expect_body(&expected_body("A")).await?;

    Ok(())
}

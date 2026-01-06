use anyhow::Result;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;

#[tokio::test]
async fn r10_startup_corrupted_lkg() -> Result<()> {
    if std::env::var("TEST_MODE").unwrap_or_default() == "docker" {
        println!("Skipping r10 in docker mode (custom LKG mount not supported)");
        return Ok(());
    }
    let temp_dir = tempfile::tempdir()?;
    let lkg_path = temp_dir.path().join("config.pvs");
    fs::write(&lkg_path, b"GARBAGE")?;

    let mut options = RelayOptions::default();
    options.lkg_path = Some(lkg_path);

    // Initial state setup bypasses PavisScenario for corruption test
    let result: Result<pavis_e2e::support::relay::RelayInstance> =
        pavis_e2e::support::relay::RelayInstance::new(options).await;
    assert!(result.is_err());

    Ok(())
}

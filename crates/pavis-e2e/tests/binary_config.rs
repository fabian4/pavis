mod common;

use anyhow::Result;
use pavis_e2e::utils::{find_project_root, get_upstream_name};
use std::fs;
use std::process::Command;

#[tokio::test]
async fn test_binary_config_loading() -> Result<()> {
    let project_root = find_project_root()?;

    // 1. Setup paths
    let yaml_src = project_root.join("crates/pavis-e2e/config/templates/basic_routing.yaml");
    let yaml_gen = project_root.join("crates/pavis-e2e/config/generated_binary_test.yaml");
    let pvs_gen = project_root.join("crates/pavis-e2e/config/generated_binary_test.pvs");

    // 2. Generate YAML from template
    let content = fs::read_to_string(&yaml_src)?;
    let content = content
        .replace("${BACKEND_V1_HOST}", "127.0.0.1")
        .replace("${BACKEND_V2_HOST}", "127.0.0.1")
        .replace("${TEST_MODE}", "binary");
    fs::write(&yaml_gen, content)?;

    // 3. Compile to .pvs using pavis-cli
    // We assume pavis-cli is built (standard for E2E tests run via cargo)
    let cli_path = project_root.join("target/debug/pavis-cli");
    if !cli_path.exists() {
        // Fallback to building it if not found (though slow)
        let status = Command::new("cargo")
            .args(["build", "-p", "pavis-cli"])
            .status()?;
        assert!(status.success(), "Failed to build pavis-cli");
    }

    let status = Command::new(&cli_path)
        .arg("compile")
        .arg("--input")
        .arg(&yaml_gen)
        .arg("--output")
        .arg(&pvs_gen)
        .status()?;

    assert!(status.success(), "pavis-cli failed to compile config");

    // 4. Start Pavis with .pvs config
    // We manually manage the env here instead of common::setup because common::setup expects a YAML name
    let pavis_path = project_root.join("target/debug/pavis");
    if !pavis_path.exists() {
        let status = Command::new("cargo")
            .args(["build", "-p", "pavis"])
            .status()?;
        assert!(status.success(), "Failed to build pavis");
    }

    println!("🚀 Starting Pavis with BINARY config ({:?})...", pvs_gen);
    let mut pavis_child = Command::new(&pavis_path)
        .arg("--config")
        .arg(&pvs_gen)
        .spawn()?;

    // 5. Wait and Test
    let client = reqwest::Client::new();
    let result = async {
        pavis_e2e::utils::wait_for_pavis(&client).await?;
        let upstream = get_upstream_name(&client, "/").await?;
        assert!(upstream.contains("backend-v"), "Should route to a backend");
        Ok::<(), anyhow::Error>(())
    }
    .await;

    // 6. Cleanup
    let _ = pavis_child.kill();
    let _ = pavis_child.wait();
    let _ = fs::remove_file(&yaml_gen);
    let _ = fs::remove_file(&pvs_gen);

    result
}

use anyhow::Result;
use pavis_e2e::support::relay::{RelayEnv, RelayOptions};

use super::support::{
    PavisEnv, expected_body, pavis_target, runtime_config, upstreams, wait_for_body,
};

#[tokio::test]
async fn integrated_file_ingest_pipeline() -> Result<()> {
    // 1. Setup Relay with file ingest enabled
    let options = RelayOptions {
        enable_file_ingest: true,
        ingest_debounce_ms: 200,
        ..Default::default()
    };
    let relay = RelayEnv::new(options).await?;
    let ingest_path = relay.work_dir.join("input.yaml");

    // Ensure the file exists before starting
    std::fs::write(&ingest_path, "")?;

    let Some(upstreams) = upstreams().await? else {
        return Ok(());
    };
    let target = pavis_target()?;

    // 2. Setup Runtime
    // Initially connected to relay, which has no config yet (empty file) or invalid.
    // Pavis will start with LKG (v0).
    let config_initial = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );
    let pavis = PavisEnv::new(&config_initial, target.host_port, relay.base_url())?;
    let expected_a = expected_body("A");
    wait_for_body(pavis.base_url(), &expected_a).await?;

    // 3. Write v1 config to file (YAML format)
    let yaml_v1_b = format!(
        r#"
server:
  listen_addr: "{}"
upstreams:
  - name: "upstream-a"
    endpoints:
      - ip: "{}"
        port: {}
  - name: "upstream-b"
    endpoints:
      - ip: "{}"
        port: {}
routes:
  - host: "*"
    paths:
      - path: "/"
        destinations:
          - upstream: "upstream-b"
            weight: 1
"#,
        target.listen_addr,
        upstreams.a.ip(),
        upstreams.a.port(),
        upstreams.b.ip(),
        upstreams.b.port()
    );

    std::fs::write(&ingest_path, &yaml_v1_b)?;

    let expected_b = expected_body("B");
    wait_for_body(pavis.base_url(), &expected_b).await?;

    // 5. Write v2 config (Back to A)
    let yaml_v2_a = format!(
        r#"
server:
  listen_addr: "{}"
upstreams:
  - name: "upstream-a"
    endpoints:
      - ip: "{}"
        port: {}
  - name: "upstream-b"
    endpoints:
      - ip: "{}"
        port: {}
routes:
  - host: "*"
    paths:
      - path: "/"
        destinations:
          - upstream: "upstream-a"
            weight: 1
"#,
        target.listen_addr,
        upstreams.a.ip(),
        upstreams.a.port(),
        upstreams.b.ip(),
        upstreams.b.port()
    );
    std::fs::write(&ingest_path, &yaml_v2_a)?;

    wait_for_body(pavis.base_url(), &expected_body("A")).await?;

    Ok(())
}

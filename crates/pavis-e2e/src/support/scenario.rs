use crate::support::configs::runtime_config;
use crate::support::pick_port;
use anyhow::Result;
use pavis_core::RuntimeConfig;
use reqwest::Client;
use std::fs;
use std::time::{Duration, Instant};
use tokio::time::sleep;

use super::pavis::TestEnv;
use super::relay::{RelayInstance, RelayOptions};
use super::upstream::UpstreamSet;

pub struct PavisScenario {
    pub relay: RelayInstance,
    pub pavis: Option<TestEnv>,
    pub upstreams: UpstreamSet,
}

impl PavisScenario {
    pub async fn new(options: RelayOptions, with_pavis: bool) -> Result<Self> {
        let upstreams = UpstreamSet::new().await?;
        let relay = RelayInstance::new(options).await?;
        let status = relay.client().status().await?;
        println!("DEBUG: Relay started at version {}", status.version);
        let mut pavis = None;

        if with_pavis {
            let pavis_port = pick_port()?;

            if relay.env.options.enable_file_ingest {
                let listen_addr = format!("127.0.0.1:{pavis_port}").parse()?;
                let config = runtime_config(
                    listen_addr,
                    ("upstream-a", upstreams.a),
                    ("upstream-b", upstreams.b),
                    "upstream-a",
                );
                let yaml = crate::support::pvs::to_yaml(&config);
                let path = relay.ingest_path.as_ref().expect("ingest path");
                fs::write(path, yaml)?;
                sleep(Duration::from_millis(500)).await;
            }

            let env = TestEnv::new_with_relay(relay.env.base_url().to_string(), pavis_port).await?;
            pavis = Some(env);
        }

        Ok(Self {
            relay,
            pavis,
            upstreams,
        })
    }

    /// Applies a configuration by writing YAML to the Relay's ingest directory.
    pub async fn apply_config(&self, config: &RuntimeConfig) -> Result<()> {
        let yaml = crate::support::pvs::to_yaml(config);
        let path = self
            .relay
            .ingest_path
            .as_ref()
            .expect("relay ingest path not configured");

        fs::write(path, yaml)?;

        // Give some time for ingestion (debounce is 100ms by default)
        sleep(Duration::from_millis(500)).await;
        Ok(())
    }

    /// Wait for relay to reach a specific version
    pub async fn wait_for_relay_version(&self, version: u64) -> Result<()> {
        let client = self.relay.client();
        for _ in 0..50 {
            let status = client.status().await?;
            if status.version >= version {
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }
        anyhow::bail!("Timeout waiting for relay version {}", version)
    }

    /// Wait for Pavis to reach a specific version via Relay
    pub async fn wait_for_pavis_version(&self, version: u64) -> Result<()> {
        let pavis = self
            .pavis
            .as_ref()
            .expect("Pavis not started in this scenario");
        pavis.wait_for_version(version).await
    }

    /// Expect the body of the response from Pavis to contain the expected string.
    #[allow(clippy::collapsible_if)]
    pub async fn expect_body(&self, expected: &str) -> Result<()> {
        let pavis = self
            .pavis
            .as_ref()
            .expect("Pavis not started in this scenario");
        let base_url = pavis.base_url();
        let client = Client::builder().timeout(Duration::from_secs(3)).build()?;
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            if Instant::now() > deadline {
                return Err(anyhow::anyhow!("timeout waiting for response {expected}"));
            }
            if let Ok(resp) = client.get(format!("{base_url}/")).send().await {
                if let Ok(text) = resp.text().await {
                    if text.contains(expected) {
                        return Ok(());
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}

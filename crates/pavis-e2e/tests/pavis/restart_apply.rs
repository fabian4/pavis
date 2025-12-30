use anyhow::{Context, Result};
use pavis_e2e::support::{
    find_binary, find_project_root, generate_pvs, get_upstream_name, wait_for_pavis,
};
use reqwest::Client;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn unique_path(prefix: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{nanos}.{ext}"))
}

fn pavis_bin() -> Result<PathBuf> {
    let project_root = find_project_root()?;
    find_binary(&project_root, "pavis")
}

fn pavctl_bin() -> Result<PathBuf> {
    let project_root = find_project_root()?;
    find_binary(&project_root, "pavctl")
}

fn backend_host(env_key: &str) -> String {
    env::var(env_key).unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn write_config(path: &Path, upstream_name: &str, host: &str, port: u16) -> Result<()> {
    let yaml = format!(
        r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry: {{}}
upstreams:
  - name: "{upstream_name}"
    endpoints:
      - ip: "{host}"
        port: {port}
routes:
  - host: "*"
    paths:
      - path: "/"
        destinations:
          - upstream: "{upstream_name}"
            weight: 1
"#
    );
    std::fs::write(path, yaml).context("write config")?;
    Ok(())
}

struct PavisProcess {
    child: Child,
}

impl Drop for PavisProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn spawn_pavis(pvs_path: &Path) -> Result<PavisProcess> {
    let pavis = pavis_bin()?;
    let child = Command::new(&pavis)
        .arg("--config")
        .arg(pvs_path)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .context("spawn pavis")?;

    let client = Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .context("build client")?;
    wait_for_pavis(&client).await?;

    Ok(PavisProcess { child })
}

#[tokio::test]
async fn pavis_applies_config_on_restart() -> Result<()> {
    let pavctl = pavctl_bin()?;

    let config_v1 = unique_path("pavis_restart_v1", "yaml");
    let config_v2 = unique_path("pavis_restart_v2", "yaml");
    let pvs_v1 = unique_path("pavis_restart_v1", "pvs");
    let pvs_v2 = unique_path("pavis_restart_v2", "pvs");

    let host_v1 = backend_host("BACKEND_V1_HOST");
    let host_v2 = backend_host("BACKEND_V2_HOST");

    write_config(&config_v1, "backend-v1", &host_v1, 8081)?;
    write_config(&config_v2, "backend-v2", &host_v2, 8082)?;

    generate_pvs(&pavctl, &config_v1, &pvs_v1)?;
    generate_pvs(&pavctl, &config_v2, &pvs_v2)?;

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("build client")?;

    {
        let _proc = spawn_pavis(&pvs_v1).await?;
        let upstream = get_upstream_name(&client, "/").await?;
        assert_eq!(upstream, "backend-v1");
    }

    {
        let _proc = spawn_pavis(&pvs_v2).await?;
        let upstream = get_upstream_name(&client, "/").await?;
        assert_eq!(upstream, "backend-v2");
    }

    let _ = std::fs::remove_file(&config_v1);
    let _ = std::fs::remove_file(&config_v2);
    let _ = std::fs::remove_file(&pvs_v1);
    let _ = std::fs::remove_file(&pvs_v2);

    Ok(())
}

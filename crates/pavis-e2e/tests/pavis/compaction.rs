use anyhow::{Context, Result};
use bytes::Bytes;
use pavis_codec_api::{Codec, CompactionLevel};
use pavis_codec_serde::{SerdeCodec, SerdeFormat};
use pavis_e2e::support::{
    find_binary, find_project_root, get_upstream_name, resolve_docker_service_ip, wait_for_pavis,
};
use pavis_ingest_api::{Artifact, Format, SourceInfo};
use reqwest::Client;
use std::env;
use std::fs;
use std::net::IpAddr;
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

fn backend_host(env_key: &str) -> String {
    env::var(env_key).unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn resolve_backend_host(host: String) -> Result<String> {
    if matches!(test_mode(), TestMode::Docker) && host.parse::<IpAddr>().is_err() {
        let project_root = find_project_root()?;
        resolve_docker_service_ip(&project_root, &host)
    } else {
        Ok(host)
    }
}

fn write_config(path: &Path, host: &str, port: u16) -> Result<()> {
    let yaml = format!(
        r#"
listeners:
  - name: "default"
    listen_addr: "0.0.0.0:8080"
telemetry: {{}}
upstreams:
  - name: "backend-v1"
    endpoints:
      - ip: "{host}"
        port: {port}
routes:
  - host: "*"
    paths:
      - path: "/known"
        destinations:
          - upstream: "backend-v1"
            weight: 1
"#
    );
    std::fs::write(path, yaml).context("write config")?;
    Ok(())
}

fn write_pvs_with_compaction(
    yaml_path: &Path,
    output_path: &Path,
    level: CompactionLevel,
) -> Result<()> {
    let bytes = std::fs::read(yaml_path).context("read config")?;
    let artifact = Artifact::new(Bytes::from(bytes), Format::Yaml, SourceInfo::unknown());
    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };
    let validated = codec
        .materialize(artifact, level)
        .context("materialize config")?;
    pavis_pvs::write(output_path, &validated.into_inner()).context("write pvs")?;
    Ok(())
}

struct PavisProcess {
    child: Option<Child>,
}

impl Drop for PavisProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

enum TestMode {
    Binary,
    Docker,
}

fn test_mode() -> TestMode {
    match env::var("TEST_MODE")
        .unwrap_or_else(|_| "binary".to_string())
        .as_str()
    {
        "docker" => TestMode::Docker,
        _ => TestMode::Binary,
    }
}

async fn spawn_pavis(pvs_path: &Path) -> Result<PavisProcess> {
    let mut child = None;
    match test_mode() {
        TestMode::Binary => {
            let pavis = pavis_bin()?;
            let process = Command::new(&pavis)
                .arg("--config")
                .arg(pvs_path)
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .context("spawn pavis")?;
            child = Some(process);
        }
        TestMode::Docker => {
            let project_root = find_project_root()?;
            let shared_config = project_root.join("crates/pavis-e2e/config/generated_config.pvs");
            fs::copy(pvs_path, &shared_config).context("copy pvs")?;
            let compose_file =
                project_root.join("crates/pavis-e2e/config/docker-compose-pavis.yaml");
            let status = Command::new("docker")
                .args([
                    "compose",
                    "-f",
                    compose_file.to_str().expect("valid path"),
                    "up",
                    "-d",
                    "--force-recreate",
                    "pavis",
                ])
                .status()?;
            if !status.success() {
                return Err(anyhow::anyhow!("Failed to restart docker container"));
            }
        }
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .context("build client")?;
    wait_for_pavis(&client, "http://localhost:8080").await?;

    Ok(PavisProcess { child })
}

async fn routing_snapshot(client: &Client) -> Result<(u16, u16)> {
    let known = get_upstream_name(client, "/known").await?;
    assert_eq!(known, "backend-v1");

    let unknown_status = client
        .get("http://localhost:8080/unknown")
        .send()
        .await?
        .status()
        .as_u16();
    let health_status = client
        .get("http://localhost:8080/health")
        .send()
        .await?
        .status()
        .as_u16();
    let ready_status = client
        .get("http://localhost:8080/ready")
        .send()
        .await?
        .status()
        .as_u16();
    assert_eq!(unknown_status, 404);

    Ok((health_status, ready_status))
}

#[tokio::test]
async fn compaction_preserves_routing_semantics() -> Result<()> {
    let config_path = unique_path("pavis_compaction", "yaml");
    let host = resolve_backend_host(backend_host("BACKEND_V1_HOST"))?;
    write_config(&config_path, &host, 8081)?;

    let pvs_off = unique_path("pavis_compaction_off", "pvs");
    let pvs_trim = unique_path("pavis_compaction_trim", "pvs");
    let pvs_prune = unique_path("pavis_compaction_prune", "pvs");

    write_pvs_with_compaction(&config_path, &pvs_off, CompactionLevel::Off)?;
    write_pvs_with_compaction(&config_path, &pvs_trim, CompactionLevel::Trim)?;
    write_pvs_with_compaction(&config_path, &pvs_prune, CompactionLevel::Prune)?;

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("build client")?;

    let baseline = {
        let _proc = spawn_pavis(&pvs_off).await?;
        routing_snapshot(&client).await?
    };

    for pvs in [&pvs_trim, &pvs_prune] {
        let _proc = spawn_pavis(pvs).await?;
        let snapshot = routing_snapshot(&client).await?;
        assert_eq!(snapshot, baseline);
    }

    let _ = std::fs::remove_file(&config_path);
    let _ = std::fs::remove_file(&pvs_off);
    let _ = std::fs::remove_file(&pvs_trim);
    let _ = std::fs::remove_file(&pvs_prune);

    Ok(())
}

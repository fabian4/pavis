use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub fn resolve_docker_service_ip(project_root: &Path, service: &str) -> Result<String> {
    let compose_file = std::env::var("PAVIS_COMPOSE_FILE")
        .map(|value| Path::new(&value).to_path_buf())
        .unwrap_or_else(|_| project_root.join("crates/pavis-e2e/config/docker-compose-pavis.yaml"));
    let mut args = vec![
        "compose".to_string(),
        "-f".to_string(),
        compose_file
            .to_str()
            .context("docker compose path is not valid UTF-8")?
            .to_string(),
    ];
    if let Ok(project) = std::env::var("PAVIS_COMPOSE_PROJECT") {
        args.push("-p".to_string());
        args.push(project);
    }
    args.push("ps".to_string());
    args.push("-q".to_string());
    args.push(service.to_string());

    let output = Command::new("docker")
        .args(&args)
        .output()
        .context("Failed to run docker compose ps")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to resolve docker container for service '{service}'"
        ));
    }

    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if container_id.is_empty() {
        return Err(anyhow::anyhow!(
            "No container found for docker service '{service}'"
        ));
    }

    let output = Command::new("docker")
        .args([
            "inspect",
            "-f",
            "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}",
            &container_id,
        ])
        .output()
        .context("Failed to run docker inspect")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to inspect docker container for service '{service}'"
        ));
    }

    let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if ip.is_empty() {
        return Err(anyhow::anyhow!(
            "No IP found for docker service '{service}'"
        ));
    }

    Ok(ip)
}

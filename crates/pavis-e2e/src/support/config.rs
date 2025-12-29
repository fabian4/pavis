use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use super::tls::resolve_docker_service_ip;

pub(super) fn generate_config(
    project_root: &Path,
    config_name: &str,
    mode: &str,
) -> Result<PathBuf> {
    let config_src = project_root
        .join("crates/pavis-e2e/config/templates")
        .join(config_name);

    let config_dest = project_root
        .join("crates/pavis-e2e/config")
        .join(format!("generated_{config_name}"));

    let backend_v1 = env::var("BACKEND_V1_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let backend_v2 = env::var("BACKEND_V2_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let backend_v1 = resolve_docker_host_if_needed(mode, project_root, backend_v1)?;
    let backend_v2 = resolve_docker_host_if_needed(mode, project_root, backend_v2)?;

    let content = fs::read_to_string(&config_src)
        .with_context(|| format!("Failed to read config: {config_src:?}"))?;

    let content = content
        .replace("${BACKEND_V1_HOST}", &backend_v1)
        .replace("${BACKEND_V2_HOST}", &backend_v2)
        .replace("${TEST_MODE}", mode);

    fs::write(&config_dest, content)?;
    Ok(config_dest)
}

fn resolve_docker_host_if_needed(mode: &str, project_root: &Path, host: String) -> Result<String> {
    if mode == "docker" && host.parse::<IpAddr>().is_err() {
        resolve_docker_service_ip(project_root, &host)
    } else {
        Ok(host)
    }
}

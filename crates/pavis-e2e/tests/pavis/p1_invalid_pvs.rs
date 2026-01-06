use anyhow::{Context, Result};
use pavis_e2e::support::{find_binary, find_project_root};
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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

#[test]
fn pavis_rejects_invalid_pvs() -> Result<()> {
    let pavis = pavis_bin()?;
    let pvs_path = unique_path("pavis_invalid", "pvs");
    std::fs::write(&pvs_path, b"not-a-pvs").context("write invalid pvs")?;

    let output = Command::new(&pavis)
        .arg("--config")
        .arg(&pvs_path)
        .output()
        .context("run pavis")?;

    let _ = std::fs::remove_file(&pvs_path);
    assert!(!output.status.success());

    Ok(())
}

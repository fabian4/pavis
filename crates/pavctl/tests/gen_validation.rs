use anyhow::Result;
use pavis_codec_serde::config::{Listener, SerdeConfig, TlsConfig};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn gen_warns_on_missing_cert_paths() -> Result<()> {
    let pavctl_bin = pavctl_bin();

    // 1. Setup config with missing paths
    let config = SerdeConfig {
        listeners: Some(vec![Listener {
            name: "tls-listener".to_string(),
            address: "127.0.0.1:8443".to_string(),
            workers: None,
            tls: Some(TlsConfig {
                cert_path: Some("/tmp/non-existent-cert.pem".to_string()),
                key_path: Some("/tmp/non-existent-key.pem".to_string()),
                client_auth: None,
            }),
        }]),
        ..Default::default()
    };

    let input_file = tempfile::Builder::new().suffix(".yaml").tempfile()?;
    let pvs_file = tempfile::Builder::new().suffix(".pvs").tempfile()?;

    let yaml = serde_yaml::to_string(&config)?;
    fs::write(input_file.path(), yaml)?;

    // 2. Run pavctl gen and capture output
    let output = Command::new(&pavctl_bin)
        .args([
            "gen",
            input_file.path().to_str().unwrap(),
            pvs_file.path().to_str().unwrap(),
        ])
        .output()?;

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("Warning: Certificate file not found locally: /tmp/non-existent-cert.pem")
    );
    assert!(stderr.contains("Warning: Key file not found locally: /tmp/non-existent-key.pem"));

    Ok(())
}

fn pavctl_bin() -> PathBuf {
    let mut dir = std::env::current_dir().expect("cwd");
    loop {
        if dir.join("Cargo.lock").exists() {
            break;
        }
        if !dir.pop() {
            panic!("Could not find workspace root");
        }
    }

    let bin_path = dir.join("target/debug/pavctl");
    if !bin_path.exists() {
        let status = Command::new("cargo")
            .args(["build", "-p", "pavctl"])
            .current_dir(&dir)
            .status()
            .expect("spawn cargo build");
        assert!(status.success(), "cargo build -p pavctl failed");
    }
    bin_path
}

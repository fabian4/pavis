use anyhow::{Context, Result};
use pavis_codec_serde::SerdeFormat;
use pavis_codec_serde::config::{
    Listener, Matcher, Route, RouteAction, SerdeConfig, Upstream, VirtualHost, WeightedDestination,
};
use pavis_core::{Discovery, HttpVersion, LoadBalancer};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn pipeline_yaml_to_yaml() -> Result<()> {
    run_pipeline_test(SerdeFormat::Yaml, SerdeFormat::Yaml)
}

#[test]
fn pipeline_yaml_to_json() -> Result<()> {
    run_pipeline_test(SerdeFormat::Yaml, SerdeFormat::Json)
}

#[test]
fn pipeline_json_to_yaml() -> Result<()> {
    run_pipeline_test(SerdeFormat::Json, SerdeFormat::Yaml)
}

#[test]
fn pipeline_json_to_json() -> Result<()> {
    run_pipeline_test(SerdeFormat::Json, SerdeFormat::Json)
}

fn run_pipeline_test(input_format: SerdeFormat, output_format: SerdeFormat) -> Result<()> {
    let pavctl_bin = pavctl_bin();
    let config = sample_config();

    // 1. Write Input
    let input_ext = format_ext(input_format);
    let input_file = tempfile::Builder::new()
        .suffix(&format!(".{}", input_ext))
        .tempfile()?;
    write_config(input_file.path(), input_format, &config)?;

    // 2. Gen PVS
    let pvs_file = tempfile::Builder::new().suffix(".pvs").tempfile()?;
    // We pass paths as strings. Note: NamedTempFile deletes on drop, so we keep the objects alive.
    // The CLI takes paths.
    run_pavctl(
        &pavctl_bin,
        &[
            "gen",
            input_file.path().to_str().unwrap(),
            pvs_file.path().to_str().unwrap(),
        ],
    )?;

    // 3. Convert PVS -> Output
    let output_ext = format_ext(output_format);
    let out_file = tempfile::Builder::new()
        .suffix(&format!(".{}", output_ext))
        .tempfile()?;
    run_pavctl(
        &pavctl_bin,
        &[
            "convert",
            pvs_file.path().to_str().unwrap(),
            out_file.path().to_str().unwrap(),
        ],
    )?;

    // 4. Validate
    let converted = fs::read_to_string(out_file.path())?;
    assert_converted_matches_canonical(&converted, output_format, &config)?;

    Ok(())
}

fn format_ext(format: SerdeFormat) -> &'static str {
    match format {
        SerdeFormat::Yaml => "yaml",
        SerdeFormat::Json => "json",
    }
}

fn run_pavctl(bin: &Path, args: &[&str]) -> Result<()> {
    if !bin.exists() {
        anyhow::bail!("Binary not found at path: {:?}", bin);
    }

    let output = Command::new(bin)
        .args(args)
        .output()
        .with_context(|| format!("Failed to execute pavctl {:?} with args {:?}", bin, args))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "pavctl {:?} failed with status: {}\nSTDOUT:\n{}\nSTDERR:\n{}",
            args,
            output.status,
            stdout,
            stderr
        );
    }
    Ok(())
}

fn sample_config() -> SerdeConfig {
    SerdeConfig {
        listeners: Some(vec![Listener {
            name: "default".to_string(),
            address: "127.0.0.1:8080".to_string(),
            workers: None,
            tls: None,
        }]),
        telemetry: Some(Default::default()),
        upstreams: Some(vec![Upstream {
            id: None,
            name: "backend".to_string(),
            discovery: Some(Discovery::Static),
            balancer: Some(LoadBalancer::Random),
            protocol: Some(HttpVersion::H1),
            pool: None,
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![pavis_codec_serde::config::Endpoint {
                address: "127.0.0.1".to_string(),
                port: 8081,
                weight: Some(1),
            }],
        }]),
        routes: Some(vec![VirtualHost {
            host: "example.com".to_string(),
            paths: vec![Route {
                matcher: Some(Matcher::Prefix {
                    path: "/".to_string(),
                }),
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                principal: None,
                rewrite: None,
                action: RouteAction::Forward {
                    destinations: vec![WeightedDestination {
                        upstream: "backend".to_string(),
                        weight: 1,
                    }],
                },
            }],
        }]),
    }
}

fn assert_converted_matches_canonical(
    converted: &str,
    format: SerdeFormat,
    config: &SerdeConfig,
) -> Result<()> {
    let runtime = config.clone().build()?;
    let canonical: SerdeConfig = runtime.into();

    match format {
        SerdeFormat::Yaml => {
            let converted_value: serde_yaml::Value =
                serde_yaml::from_str(converted).context("parse converted yaml")?;
            let expected_value =
                serde_yaml::to_value(canonical).context("serialize canonical yaml")?;
            assert_eq!(converted_value, expected_value);
        }
        SerdeFormat::Json => {
            let converted_value: serde_json::Value =
                serde_json::from_str(converted).context("parse converted json")?;
            let expected_value =
                serde_json::to_value(canonical).context("serialize canonical json")?;
            assert_eq!(converted_value, expected_value);
        }
    }

    Ok(())
}

fn write_config(path: &Path, format: SerdeFormat, config: &SerdeConfig) -> Result<()> {
    let out = match format {
        SerdeFormat::Yaml => serde_yaml::to_string(config)?,
        SerdeFormat::Json => serde_json::to_string_pretty(config)?,
    };
    fs::write(path, out).context("write config")?;
    Ok(())
}

fn pavctl_bin() -> PathBuf {
    let env_val = std::env::var("CARGO_BIN_EXE_pavctl").ok();
    if let Some(env_val) = env_val {
        return pavctl_bin_helper(Some(env_val));
    }

    let root = workspace_root();
    ensure_pavctl_built(&root);
    debug_pavctl_path(&root)
}

fn ensure_pavctl_built(root: &Path) {
    use std::sync::OnceLock;

    static BUILT: OnceLock<()> = OnceLock::new();
    let root = root.to_path_buf();
    BUILT.get_or_init(|| {
        if debug_pavctl_path(&root).exists() {
            return;
        }

        let status = Command::new("cargo")
            .args(["build", "-p", "pavctl"])
            .current_dir(&root)
            .status()
            .expect("spawn cargo build");
        assert!(status.success(), "cargo build -p pavctl failed");
    });
}

fn workspace_root() -> PathBuf {
    let mut dir = std::env::current_dir().expect("cwd");
    loop {
        if dir.join("Cargo.lock").exists() {
            break;
        }
        if !dir.pop() {
            panic!("Could not find workspace root");
        }
    }

    dir
}

fn debug_pavctl_path(root: &Path) -> PathBuf {
    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(target_dir).join("debug/pavctl");
    }
    root.join("target/debug/pavctl")
}

fn pavctl_bin_helper(env_val: Option<String>) -> PathBuf {
    if let Some(path) = env_val {
        let path = PathBuf::from(path);

        if path.exists() {
            return path;
        }
    }

    pavctl_bin_from(std::env::current_dir().expect("cwd"))
}

fn pavctl_bin_from(mut dir: PathBuf) -> PathBuf {
    // Check CARGO_TARGET_DIR override (common in CI)

    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        let target_path = PathBuf::from(target_dir);

        let release = target_path.join("release/pavctl");

        if release.exists() {
            return release;
        }

        let debug = target_path.join("debug/pavctl");

        if debug.exists() {
            return debug;
        }
    }

    loop {
        if dir.join("Cargo.lock").exists() {
            break;
        }

        if !dir.pop() {
            panic!("Could not find workspace root");
        }
    }

    // Prefer release binary if it exists (common in CI after build step)

    let release_path = dir.join("target/release/pavctl");

    if release_path.exists() {
        return release_path;
    }

    let debug_path = dir.join("target/debug/pavctl");

    if debug_path.exists() {
        return debug_path;
    }

    panic!("Binary pavctl not found; run cargo build -p pavctl");
}

#[test]

fn pavctl_bin_prefers_env_override() {
    let temp = tempfile::Builder::new()
        .suffix(".bin")
        .tempfile()
        .expect("tempfile");

    let path = temp.path().to_owned();

    // Test the logic without touching the actual global environment

    let resolved = pavctl_bin_helper(Some(path.to_string_lossy().to_string()));

    assert_eq!(resolved, path);
}

#[test]
fn pavctl_bin_finds_release_binary() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("Cargo.lock"), "").expect("lock");
    let release_dir = dir.path().join("target/release");
    std::fs::create_dir_all(&release_dir).expect("release dir");
    let release_path = release_dir.join("pavctl");
    std::fs::write(&release_path, b"").expect("release bin");

    let resolved = pavctl_bin_from(dir.path().to_owned());
    assert_eq!(resolved, release_path);
}

#[test]
fn pavctl_bin_panics_without_workspace_root() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_owned();
    let result = std::panic::catch_unwind(|| pavctl_bin_from(path));
    assert!(result.is_err());
}

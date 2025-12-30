use pavis_codec_serde::SerdeFormat;
use pavis_codec_serde::config::{
    ConnectionPoolConfig, Route, SerdeConfig, ServerConfig, Upstream, VirtualHost,
    WeightedDestination,
};
use pavis_core::{HttpVersion, LoadBalancer, MatchType};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn pipeline_handles_generated_fixtures() -> anyhow::Result<()> {
    let pavctl_bin = pavctl_bin();
    assert!(
        pavctl_bin.exists(),
        "pavctl binary not found at {:?}",
        pavctl_bin
    );

    let config = sample_config();
    for input_format in [SerdeFormat::Yaml, SerdeFormat::Json] {
        let input_ext = match input_format {
            SerdeFormat::Yaml => "yaml",
            SerdeFormat::Json => "json",
        };
        let input_path = temp_path("pavctl_input", input_ext);
        write_config(&input_path, input_format, &config)?;

        let pvs_path = temp_path("pavctl_gen", "pvs");
        let status = Command::new(&pavctl_bin)
            .arg("gen")
            .arg(&input_path)
            .arg(&pvs_path)
            .status()
            .expect("run pavctl gen");
        assert!(status.success(), "gen failed for {:?}", input_path);

        for output_format in [SerdeFormat::Yaml, SerdeFormat::Json] {
            let output_ext = match output_format {
                SerdeFormat::Yaml => "yaml",
                SerdeFormat::Json => "json",
            };
            let out_path = temp_path("pavctl_out", output_ext);
            let status = Command::new(&pavctl_bin)
                .arg("convert")
                .arg(&pvs_path)
                .arg(&out_path)
                .status()
                .expect("run pavctl convert");
            assert!(status.success(), "convert failed for {:?}", pvs_path);

            let converted = fs::read_to_string(&out_path).expect("read converted");
            assert_converted_matches_canonical(&converted, output_format, &config)?;
            let _ = fs::remove_file(&out_path);
        }

        let _ = fs::remove_file(&pvs_path);
        let _ = fs::remove_file(&input_path);
    }

    Ok(())
}

fn sample_config() -> SerdeConfig {
    SerdeConfig {
        server: ServerConfig {
            listen_addr: "127.0.0.1:8080".to_string(),
            worker_threads: None,
            tls: None,
        },
        telemetry: Default::default(),
        upstreams: vec![Upstream {
            name: "backend".to_string(),
            load_balancer: LoadBalancer::Random,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![pavis_codec_serde::config::Endpoint {
                ip: "127.0.0.1".to_string(),
                port: 8081,
                weight: Some(1),
            }],
        }],
        routes: vec![VirtualHost {
            host: "example.com".to_string(),
            paths: vec![Route {
                match_type: MatchType::Prefix,
                path: "/".to_string(),
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                destinations: vec![WeightedDestination {
                    upstream: "backend".to_string(),
                    weight: 1,
                }],
            }],
        }],
    }
}

fn assert_converted_matches_canonical(
    converted: &str,
    format: SerdeFormat,
    config: &SerdeConfig,
) -> anyhow::Result<()> {
    let runtime = config.clone().build()?;
    let canonical: SerdeConfig = runtime.into();

    match format {
        SerdeFormat::Yaml => {
            let converted_value: serde_yaml::Value =
                serde_yaml::from_str(converted).expect("parse converted yaml");
            let expected_value = serde_yaml::to_value(canonical).expect("serialize canonical yaml");
            assert_eq!(converted_value, expected_value);
        }
        SerdeFormat::Json => {
            let converted_value: serde_json::Value =
                serde_json::from_str(converted).expect("parse converted json");
            let expected_value = serde_json::to_value(canonical).expect("serialize canonical json");
            assert_eq!(converted_value, expected_value);
        }
    }

    Ok(())
}

fn write_config(path: &PathBuf, format: SerdeFormat, config: &SerdeConfig) -> anyhow::Result<()> {
    let out = match format {
        SerdeFormat::Yaml => serde_yaml::to_string(config)?,
        SerdeFormat::Json => serde_json::to_string_pretty(config)?,
    };
    fs::write(path, out).expect("write config");
    Ok(())
}

fn pavctl_bin() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_pavctl") {
        return PathBuf::from(path);
    }
    let mut dir = std::env::current_dir().expect("cwd");
    loop {
        if dir.join("Cargo.lock").exists() {
            break;
        }
        if !dir.pop() {
            panic!("Could not find workspace root");
        }
    }
    let debug_path = dir.join("target/debug/pavctl");
    if debug_path.exists() {
        return debug_path;
    }
    let release_path = dir.join("target/release/pavctl");
    if release_path.exists() {
        return release_path;
    }
    panic!("Binary pavctl not found; run cargo build -p pavctl");
}

fn temp_path(prefix: &str, ext: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{nanos}.{ext}"))
}

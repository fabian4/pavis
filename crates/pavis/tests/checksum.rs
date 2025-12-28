use pavis_core::RuntimeConfig;
use pavis_pvs as pvs;
use std::path::PathBuf;
use std::process::Command;

fn get_binary_path() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap();
    loop {
        if dir.join("Cargo.lock").exists() {
            break;
        }
        if !dir.pop() {
            panic!("Could not find project root");
        }
    }

    let debug_path = dir.join("target/debug/pavis");
    if debug_path.exists() {
        return debug_path;
    }
    let release_path = dir.join("target/release/pavis");
    if release_path.exists() {
        return release_path;
    }
    panic!("Pavis binary not found");
}

#[test]
fn test_checksum_validation_success() {
    let binary = get_binary_path();

    let config = RuntimeConfig {
        server: pavis_core::ServerConfig {
            listen_addr: "127.0.0.1:0".to_string(),
            worker_threads: None,
            tls: None,
        },
        telemetry: pavis_core::TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            prometheus_addr: None,
            access_log: pavis_core::AccessLogConfig::False,
            tracing: None,
        },
        upstreams: vec![],
        routes: vec![],
    };

    let temp_dir = std::env::temp_dir();
    let config_path = temp_dir.join("pavis_checksum_ok.pvs");
    pvs::write(&config_path, &config).expect("Failed to write config");

    // It should start successfully (and then we kill it)
    let mut child = Command::new(binary)
        .arg("--config")
        .arg(&config_path)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("Failed to spawn process");

    std::thread::sleep(std::time::Duration::from_secs(1));

    // If it's still running, it passed validation
    match child.try_wait() {
        Ok(None) => {
            let _ = child.kill();
        }
        Ok(Some(status)) => {
            // It might have exited if listen port 0 caused immediate exit or something else,
            // but for now we assume it runs forever.
            // Actually, listen_addr 0 might fail binding if not handled?
            // But let's assume it works or fails later.
            // If it failed with checksum error, it would be exit code 1.
            if !status.success() {
                panic!("Process exited unexpectedly");
            }
        }
        Err(_) => {}
    }
    let _ = std::fs::remove_file(config_path);
}

#[test]
fn test_checksum_validation_failure() {
    let binary = get_binary_path();

    let config = RuntimeConfig {
        server: pavis_core::ServerConfig {
            listen_addr: "127.0.0.1:0".to_string(),
            worker_threads: None,
            tls: None,
        },
        telemetry: pavis_core::TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            prometheus_addr: None,
            access_log: pavis_core::AccessLogConfig::False,
            tracing: None,
        },
        upstreams: vec![],
        routes: vec![],
    };

    let temp_dir = std::env::temp_dir();
    let config_path = temp_dir.join("pavis_checksum_fail.pvs");
    pvs::write(&config_path, &config).expect("Failed to write config");

    let mut bytes = std::fs::read(&config_path).expect("Failed to read config");
    if bytes.len() <= pvs::HEADER_SIZE {
        panic!("Expected payload to be present for corruption");
    }
    bytes[pvs::HEADER_SIZE] ^= 0xFF;
    std::fs::write(&config_path, bytes).expect("Failed to write corrupted config");

    let output = Command::new(binary)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Checksum mismatch"));

    let _ = std::fs::remove_file(config_path);
}

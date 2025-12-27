use pavis_core::{PAVIS_MAGIC, PAVIS_VERSION, PavisHeader, RuntimeConfig};
use rkyv::ser::{Serializer, serializers::AllocSerializer};
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
        header: PavisHeader::default(),
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

    let mut serializer = AllocSerializer::<1024>::default();
    serializer.serialize_value(&config).unwrap();
    let bytes = serializer.into_serializer().into_inner();

    let checksum = pavis_core::compute_checksum(&bytes);

    let header = PavisHeader {
        magic: *PAVIS_MAGIC,
        version: PAVIS_VERSION,
        algorithm: 1,
        checksum,
        _reserved: [0; 20],
    };

    let mut final_bytes = Vec::new();
    final_bytes.extend_from_slice(&header.magic);
    final_bytes.extend_from_slice(&header.version.to_le_bytes());
    final_bytes.extend_from_slice(&header.algorithm.to_le_bytes());
    final_bytes.extend_from_slice(&header.checksum);
    final_bytes.extend_from_slice(&header._reserved);
    final_bytes.extend_from_slice(&bytes);

    let temp_dir = std::env::temp_dir();
    let config_path = temp_dir.join("pavis_checksum_ok.pvs");
    std::fs::write(&config_path, final_bytes).expect("Failed to write config");

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
        header: PavisHeader::default(),
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

    let mut serializer = AllocSerializer::<1024>::default();
    serializer.serialize_value(&config).unwrap();
    let mut bytes = serializer.into_serializer().into_inner().to_vec();

    // Compute valid checksum for original bytes
    let checksum = pavis_core::compute_checksum(&bytes);

    // Corrupt the payload
    if !bytes.is_empty() {
        bytes[0] ^= 0xFF;
    }

    let header = PavisHeader {
        magic: *PAVIS_MAGIC,
        version: PAVIS_VERSION,
        algorithm: 1,
        checksum, // Checksum matches ORIGINAL bytes, not corrupted ones
        _reserved: [0; 20],
    };

    let mut final_bytes = Vec::new();
    final_bytes.extend_from_slice(&header.magic);
    final_bytes.extend_from_slice(&header.version.to_le_bytes());
    final_bytes.extend_from_slice(&header.algorithm.to_le_bytes());
    final_bytes.extend_from_slice(&header.checksum);
    final_bytes.extend_from_slice(&header._reserved);
    final_bytes.extend_from_slice(&bytes);

    let temp_dir = std::env::temp_dir();
    let config_path = temp_dir.join("pavis_checksum_fail.pvs");
    std::fs::write(&config_path, final_bytes).expect("Failed to write config");

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

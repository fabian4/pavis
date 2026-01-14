use pavis_core::{AccessLogPolicy, Metrics, RuntimeConfig, ServiceName, Telemetry, WorkerCount};
use pavis_pvs as pvs;
use std::io::BufRead;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn minimal_config() -> RuntimeConfig {
    RuntimeConfig {
        listeners: vec![pavis_core::Listener {
            name: pavis_core::ListenerName("default".to_string()),
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0),
            workers: WorkerCount::Auto,
            tls: pavis_core::TlsConfig::Disabled,
        }],
        telemetry: Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName("pavis".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: pavis_core::TracingPolicy::Disabled,
        },
        upstreams: vec![],
        routes: vec![],
    }
}

fn get_binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_pavis") {
        return PathBuf::from(path);
    }

    let mut dir = std::env::current_dir().unwrap();
    loop {
        if dir.join("Cargo.lock").exists() {
            break;
        }
        if !dir.pop() {
            panic!("Could not find project root");
        }
    }

    let debug_path = with_bin_extension(dir.join("target/debug/pavis"));
    if debug_path.exists() {
        return debug_path;
    }
    let release_path = with_bin_extension(dir.join("target/release/pavis"));
    if release_path.exists() {
        return release_path;
    }
    panic!("Pavis binary not found");
}

fn with_bin_extension(path: PathBuf) -> PathBuf {
    if cfg!(windows) {
        path.with_extension("exe")
    } else {
        path
    }
}

fn wait_for_log_line(child: &mut Child, needle: &str, timeout: Duration) {
    let start = Instant::now();
    let needle = needle.to_string();
    let (tx, rx) = mpsc::channel();
    let spawn_reader = |handle: Box<dyn std::io::Read + Send>| {
        let tx = tx.clone();
        let thread_needle = needle.clone();
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(handle);
            let mut line = String::new();
            while let Ok(bytes) = reader.read_line(&mut line) {
                if bytes == 0 {
                    break;
                }
                if line.contains(&thread_needle) {
                    let _ = tx.send(());
                    break;
                }
                line.clear();
            }
        });
    };

    if let Some(stdout) = child.stdout.take() {
        spawn_reader(Box::new(stdout));
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_reader(Box::new(stderr));
    }
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            panic!("Process exited before listen: {}", status);
        }
        if rx.try_recv().is_ok() {
            return;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            panic!("Timed out waiting for log line '{}'", needle);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn test_checksum_valid_pvs_starts() {
    let binary = get_binary_path();

    let config = minimal_config();

    let temp_dir = std::env::temp_dir();
    let config_path = temp_dir.join("pavis_checksum_ok.pvs");
    pvs::write(&config_path, &config).expect("Failed to write config");

    // It should start successfully (and then we kill it)
    let mut child = Command::new(binary)
        .arg("--config")
        .arg(&config_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn process");

    wait_for_log_line(&mut child, "Pavis starting", Duration::from_secs(5));

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
fn test_checksum_corrupt_payload_rejected() {
    let binary = get_binary_path();

    let config = minimal_config();

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

#[test]
fn test_checksum_truncated_payload_rejected() {
    let binary = get_binary_path();

    let config = minimal_config();

    let temp_dir = std::env::temp_dir();
    let config_path = temp_dir.join(format!(
        "pavis_checksum_truncated_{}.pvs",
        std::process::id()
    ));
    pvs::write(&config_path, &config).expect("Failed to write config");

    let mut bytes = std::fs::read(&config_path).expect("Failed to read config");
    let payload = bytes[pvs::HEADER_SIZE..].to_vec();
    assert!(payload.len() > 5, "expected payload larger than truncation");
    let truncated_payload = &payload[..5];
    let checksum = pvs::compute_checksum(truncated_payload);
    bytes.truncate(pvs::HEADER_SIZE);
    bytes[12..44].copy_from_slice(&checksum);
    bytes.extend_from_slice(truncated_payload);
    std::fs::write(&config_path, bytes).expect("Failed to write truncated config");

    let output = Command::new(binary)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Binary integrity check failed"));

    let _ = std::fs::remove_file(config_path);
}

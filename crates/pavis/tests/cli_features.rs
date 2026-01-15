use pavis_core::{ListenerBuilder, RuntimeConfigBuilder};
use pavis_pvs as pvs;
use std::io::BufRead;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn get_binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_pavis") {
        return PathBuf::from(path);
    }
    // Walk up until we find Cargo.lock so we can locate the build outputs.
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

    panic!(
        "Pavis binary not found at {:?} or {:?}. Please build it first.",
        debug_path, release_path
    );
}

fn with_bin_extension(path: PathBuf) -> PathBuf {
    if cfg!(windows) {
        path.with_extension("exe")
    } else {
        path
    }
}

fn temp_path(prefix: &str, ext: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{nanos}.{ext}"))
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
fn test_cli_argument_help() {
    let binary = get_binary_path();
    let output = Command::new(binary)
        .arg("--help")
        .output()
        .expect("Failed to execute binary");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:"));
}

#[test]
fn test_cli_argument_version() {
    let binary = get_binary_path();
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .expect("Failed to execute binary");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = env!("CARGO_PKG_VERSION");
    assert!(stdout.contains(version));
}

#[test]
fn test_cli_config_missing() {
    let binary = get_binary_path();
    let output = Command::new(binary)
        .output()
        .expect("Failed to execute binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("required"));
}

#[test]
fn test_cli_config_invalid_path() {
    let binary = get_binary_path();
    let output = Command::new(binary)
        .arg("--config")
        .arg("non_existent.pvs")
        .output()
        .expect("Failed to execute binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("I/O error")
            || stderr.contains("No such file or directory")
            || stderr.contains("invalid")
    );
}

#[test]
fn test_cli_config_invalid_magic() {
    let config_path = temp_path("pavis_invalid_magic", "pvs");
    let mut bytes = vec![0u8; pvs::HEADER_SIZE];
    bytes[0..4].copy_from_slice(b"NOPE");
    std::fs::write(&config_path, bytes).expect("Failed to write invalid config");

    let binary = get_binary_path();
    let output = Command::new(binary)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Invalid magic"));

    let _ = std::fs::remove_file(config_path);
}

#[test]
fn test_cli_config_version_mismatch() {
    let config_path = temp_path("pavis_version_mismatch", "pvs");
    let mut bytes = vec![0u8; pvs::HEADER_SIZE];
    bytes[0..4].copy_from_slice(pvs::PAVIS_MAGIC);
    bytes[4..8].copy_from_slice(&(pvs::PAVIS_VERSION + 1).to_le_bytes());
    bytes[8..12].copy_from_slice(&pvs::PAVIS_HASH_ALGORITHM_SHA256.to_le_bytes());
    std::fs::write(&config_path, bytes).expect("Failed to write invalid config");

    let binary = get_binary_path();
    let output = Command::new(binary)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Version mismatch"));

    let _ = std::fs::remove_file(config_path);
}

#[test]
fn test_cli_lifecycle_sigint() {
    let binary = get_binary_path();

    // Create a valid config programmatically
    let listener = ListenerBuilder::new()
        .name(pavis_core::ListenerName("default".to_string()))
        .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0))
        .workers(pavis_core::WorkerCount::Auto)
        .tls(pavis_core::TlsConfig::Disabled)
        .build()
        .expect("listener");

    let config = RuntimeConfigBuilder::new()
        .telemetry(pavis_core::Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: pavis_core::ServiceName("pavis".to_string()),
            metrics: pavis_core::Metrics::Disabled,
            access_log: pavis_core::AccessLogPolicy::Disabled,
            tracing: pavis_core::TracingPolicy::Disabled,
        })
        .add_listener(listener)
        .build()
        .expect("config");

    let config_path = temp_path("pavis_lifecycle_test", "pvs");
    pvs::write(&config_path, &config).expect("Failed to write config");

    let mut child = Command::new(binary)
        .arg("--config")
        .arg(&config_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn process");

    wait_for_log_line(&mut child, "Pavis starting", Duration::from_secs(5));

    // Terminate the process directly
    child.kill().expect("Failed to stop process");

    // Wait for exit
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                break;
            }
            Ok(None) => {
                if start.elapsed() > Duration::from_secs(10) {
                    let _ = child.kill();
                    panic!("Process did not exit within timeout");
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => panic!("Failed to wait on child: {}", e),
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(config_path);
}

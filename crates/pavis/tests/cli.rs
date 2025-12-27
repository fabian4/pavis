use pavis_core::{PAVIS_MAGIC, PAVIS_VERSION, PavisHeader, WireConfig};
use rkyv::ser::{Serializer, serializers::AllocSerializer};
use std::path::PathBuf;
use std::process::Command;

fn get_binary_path() -> PathBuf {
    // Try to find the binary in target/debug or target/release
    // We assume we are running from workspace root or crate root.
    // Let's try to find Cargo.toml to locate root.
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

    // Fallback: try to build it? No, that might be too slow/complex.
    panic!(
        "Pavis binary not found at {:?} or {:?}. Please build it first.",
        debug_path, release_path
    );
}

#[test]
fn test_cli_help() {
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
fn test_cli_missing_config() {
    let binary = get_binary_path();
    let output = Command::new(binary)
        .output()
        .expect("Failed to execute binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("required"));
}

#[test]
fn test_cli_invalid_config_path() {
    let binary = get_binary_path();
    let output = Command::new(binary)
        .arg("--config")
        .arg("non_existent.pvs")
        .output()
        .expect("Failed to execute binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Failed to open .pvs config file"));
}

#[cfg(unix)]
#[test]
fn test_process_lifecycle_sigint() {
    use std::thread;
    use std::time::{Duration, Instant};

    let binary = get_binary_path();

    // Create a valid config programmatically
    let config = WireConfig {
        header: PavisHeader::default(),
        listen_addr: "127.0.0.1:0".to_string(), // Random port
        upstreams: vec![],
        routes: vec![],
    };

    // Serialize to .pvs format
    let mut serializer = AllocSerializer::<1024>::default();
    serializer.serialize_value(&config).unwrap();
    let bytes = serializer.into_serializer().into_inner();

    let mut final_bytes = Vec::new();
    final_bytes.extend_from_slice(PAVIS_MAGIC);
    final_bytes.extend_from_slice(&PAVIS_VERSION.to_le_bytes());
    final_bytes.extend_from_slice(&bytes);

    let temp_dir = std::env::temp_dir();
    let config_path = temp_dir.join("pavis_lifecycle_test.pvs");
    std::fs::write(&config_path, final_bytes).expect("Failed to write config");

    let mut child = Command::new(binary)
        .arg("--config")
        .arg(&config_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("Failed to spawn process");

    // Give it a moment to start
    thread::sleep(Duration::from_secs(2));

    // Send SIGINT using kill command
    let status = Command::new("kill")
        .arg("-INT")
        .arg(child.id().to_string())
        .status()
        .expect("Failed to run kill");
    assert!(status.success());

    // Wait for exit
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                assert!(status.success(), "Process should exit successfully");
                break;
            }
            Ok(None) => {
                if start.elapsed() > Duration::from_secs(10) {
                    let _ = child.kill();
                    panic!("Process did not exit within timeout");
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => panic!("Failed to wait on child: {}", e),
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(config_path);
}

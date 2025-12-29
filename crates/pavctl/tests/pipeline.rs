use pavis_codec_serde::config as codec;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn yaml_files_match_pipeline_outputs() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let data_dir = root.join("tests/test_data");
    let entries = fs::read_dir(&data_dir).expect("read test_data");
    let pavctl_bin = pavctl_bin();
    assert!(
        pavctl_bin.exists(),
        "pavctl binary not found at {:?}",
        pavctl_bin
    );

    for entry in entries {
        let entry = entry.expect("read entry");
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
            continue;
        }

        // 1. Generate (Explicit Output to Temp)
        let pvs_path = temp_path("pavctl_gen", "pvs");
        let status = Command::new(&pavctl_bin)
            .arg("gen")
            .arg(&path)
            .arg(&pvs_path)
            .status()
            .expect("run pavctl gen");
        assert!(status.success(), "gen failed for {:?}", path);
        assert!(
            fs::metadata(&pvs_path).is_ok(),
            "expected pvs output for {:?}",
            path
        );

        // 2. Validate (Positional)
        let status = Command::new(&pavctl_bin)
            .arg("check")
            .arg(&path)
            .status()
            .expect("run pavctl check");
        assert!(status.success(), "check failed for {:?}", path);

        // 3. Inspect (Positional)
        let output = Command::new(&pavctl_bin)
            .arg("view")
            .arg(&pvs_path)
            .output()
            .expect("run pavctl view");
        assert!(output.status.success(), "view failed for {:?}", path);
        let actual_raw = String::from_utf8_lossy(&output.stdout);
        let actual = normalize_output(&actual_raw);
        let expected = expected_path(&path);
        let expected_content = fs::read_to_string(&expected).expect("read expected");
        let expected_norm = normalize_output(&expected_content);
        assert_eq!(actual, expected_norm, "view mismatch for {:?}", path);

        // 4. Convert (Positional + Manual Output)
        let out_yaml = temp_path("pavctl_out", "yaml");
        let status = Command::new(&pavctl_bin)
            .arg("convert")
            .arg(&pvs_path)
            .arg(&out_yaml)
            .status()
            .expect("run pavctl convert");
        assert!(status.success(), "convert failed for {:?}", path);

        let original_yaml = fs::read_to_string(&path).expect("read yaml");
        let converted_yaml = fs::read_to_string(&out_yaml).expect("read converted yaml");
        let runtime =
            pavctl::parse_yaml_runtime_from_bytes(original_yaml.as_bytes()).expect("parse yaml");
        let canonical_yaml: codec::SerdeConfig = runtime.into();
        let expected_value =
            serde_yaml::to_value(canonical_yaml).expect("serialize canonical yaml");
        let converted_value: serde_yaml::Value =
            serde_yaml::from_str(&converted_yaml).expect("parse converted yaml");
        assert_eq!(expected_value, converted_value, "mismatch for {:?}", path);

        let _ = fs::remove_file(&pvs_path);
        let _ = fs::remove_file(&out_yaml);
    }
}

fn expected_path(yaml_path: &Path) -> PathBuf {
    let mut expected = yaml_path.to_path_buf();
    expected.set_extension("txt");
    expected
}

fn normalize_output(input: &str) -> String {
    input
        .replace("\r\n", "\n")
        .lines()
        .map(|line| {
            if line.starts_with("Checksum: ") {
                "Checksum: <checksum>".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
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

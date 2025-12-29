mod check;
mod convert;
mod r#gen;
mod view;

use std::path::{Path, PathBuf};

pub(crate) use check::validate_config;
pub(crate) use convert::convert_to_config;
pub(crate) use r#gen::compile_config;
pub(crate) use view::inspect_config;

pub(crate) fn get_default_output(input: &Path, new_ext: &str) -> PathBuf {
    let mut out = input.to_path_buf();
    out.set_extension(new_ext);
    out
}

#[cfg(test)]
mod tests {
    use super::{
        compile_config, convert_to_config, get_default_output, inspect_config, validate_config,
    };
    use pavis_codec_serde::SerdeFormat;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path(prefix: &str, ext: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{nanos}.{ext}"))
    }

    fn write_yaml(path: &PathBuf, content: &str) {
        fs::write(path, content).expect("write yaml");
    }

    fn minimal_yaml() -> &'static str {
        r#"
server:
  listen_addr: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "example.com"
    paths:
      - path: "/"
        destinations:
          - upstream: "backend"
            weight: 1
"#
    }

    #[test]
    fn test_default_output_logic() {
        let input = PathBuf::from("config.yaml");
        assert_eq!(
            get_default_output(&input, "pvs"),
            PathBuf::from("config.pvs")
        );

        let input2 = PathBuf::from("dir/test.pvs");
        assert_eq!(
            get_default_output(&input2, "yaml"),
            PathBuf::from("dir/test.yaml")
        );
    }

    #[test]
    fn generate_inspect_and_convert_workflow() {
        let yaml_path = unique_path("pavctl_test", "yaml");
        let pvs_path = unique_path("pavctl_test", "pvs");
        let out_yaml = unique_path("pavctl_out", "yaml");

        write_yaml(&yaml_path, minimal_yaml());

        compile_config(yaml_path.clone(), pvs_path.clone()).expect("compile");
        inspect_config(pvs_path.clone(), false).expect("inspect");
        convert_to_config(pvs_path.clone(), Some(out_yaml.clone()), SerdeFormat::Yaml)
            .expect("convert");
        validate_config(out_yaml.clone()).expect("validate output");

        let _ = fs::remove_file(&yaml_path);
        let _ = fs::remove_file(&pvs_path);
        let _ = fs::remove_file(&out_yaml);
    }

    #[test]
    fn validate_rejects_unknown_upstream() {
        let yaml_path = unique_path("pavctl_bad", "yaml");
        let content = r#"
server:
  listen_addr: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "example.com"
    paths:
      - path: "/"
        destinations:
          - upstream: "missing"
            weight: 1
"#;
        write_yaml(&yaml_path, content);

        let err = validate_config(yaml_path.clone()).expect_err("should fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("unknown upstream"), "{msg}");

        let _ = fs::remove_file(&yaml_path);
    }
}

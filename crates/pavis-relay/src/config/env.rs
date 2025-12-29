use anyhow::{Context, Result};

pub(super) fn expand_env(value: &mut serde_yaml::Value) -> Result<()> {
    match value {
        serde_yaml::Value::String(s) => {
            *s = expand_env_str(s)?;
        }
        serde_yaml::Value::Sequence(seq) => {
            for item in seq {
                expand_env(item)?;
            }
        }
        serde_yaml::Value::Mapping(map) => {
            for (_, value) in map.iter_mut() {
                expand_env(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn expand_env_str(input: &str) -> Result<String> {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let remaining = &rest[start + 2..];
        let Some(end) = remaining.find('}') else {
            return Err(anyhow::anyhow!(
                "unterminated environment variable reference"
            ));
        };
        let key = &remaining[..end];
        let value =
            std::env::var(key).with_context(|| format!("missing environment variable: {key}"))?;
        out.push_str(&value);
        rest = &remaining[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

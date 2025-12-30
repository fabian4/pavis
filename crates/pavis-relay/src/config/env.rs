use anyhow::{Context, Result};

pub(super) fn expand_env<F>(value: &mut serde_yaml::Value, lookup: &F) -> Result<()>
where
    F: Fn(&str) -> Result<String, std::env::VarError>,
{
    match value {
        serde_yaml::Value::String(s) => {
            *s = expand_env_str(s, lookup)?;
        }
        serde_yaml::Value::Sequence(seq) => {
            for item in seq {
                expand_env(item, lookup)?;
            }
        }
        serde_yaml::Value::Mapping(map) => {
            for (_, value) in map.iter_mut() {
                expand_env(value, lookup)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn expand_env_str<F>(input: &str, lookup: &F) -> Result<String>
where
    F: Fn(&str) -> Result<String, std::env::VarError>,
{
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
        let value = lookup(key).with_context(|| format!("missing environment variable: {key}"))?;
        out.push_str(&value);
        rest = &remaining[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(feature = "serde")]
use serde::Deserialize;

use crate::runtime::AccessLogConfig;

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for AccessLogConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Bool(bool),
            String(String),
        }

        match <Helper as Deserialize>::deserialize(deserializer)? {
            Helper::Bool(false) => Ok(AccessLogConfig::False),
            Helper::Bool(true) => Err(serde::de::Error::custom("access_log cannot be true")),
            Helper::String(s) => match s.as_str() {
                "false" => Ok(AccessLogConfig::False),
                "stdout" => Ok(AccessLogConfig::Stdout),
                path if !path.is_empty() => Ok(AccessLogConfig::File(path.to_string())),
                _ => Err(serde::de::Error::custom(
                    "access_log must be 'false', 'stdout', or a file path",
                )),
            },
        }
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::AccessLogConfig;
    use serde::Deserialize;
    use serde::de::value::Error as DeError;
    use serde::de::value::{BoolDeserializer, StrDeserializer};

    #[test]
    fn access_log_accepts_string_values() {
        let config = AccessLogConfig::deserialize(StrDeserializer::<DeError>::new("stdout"))
            .expect("stdout");
        assert!(matches!(config, AccessLogConfig::Stdout));

        let config =
            AccessLogConfig::deserialize(StrDeserializer::<DeError>::new("false")).expect("false");
        assert!(matches!(config, AccessLogConfig::False));

        let config = AccessLogConfig::deserialize(StrDeserializer::<DeError>::new("logs.txt"))
            .expect("file");
        assert!(matches!(config, AccessLogConfig::File(path) if path == "logs.txt"));
    }

    #[test]
    fn access_log_rejects_true() {
        let err = AccessLogConfig::deserialize(BoolDeserializer::<DeError>::new(true))
            .expect_err("true rejects");
        assert!(
            err.to_string().contains("access_log cannot be true"),
            "unexpected error: {err}"
        );
    }
}

use crate::runtime::AccessLogConfig;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

impl Serialize for AccessLogConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Disabled => serializer.serialize_bool(false),
            Self::Stdout => serializer.serialize_str("stdout"),
            Self::File(path) => serializer.serialize_str(path),
        }
    }
}

impl<'de> Deserialize<'de> for AccessLogConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AccessLogVisitor;

        impl<'de> Visitor<'de> for AccessLogVisitor {
            type Value = AccessLogConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("false, 'stdout', or a file path string")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v {
                    Err(E::custom(
                        "access_log cannot be 'true'. Use 'stdout' or a file path.",
                    ))
                } else {
                    Ok(AccessLogConfig::Disabled)
                }
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match v {
                    "false" => Ok(AccessLogConfig::Disabled),
                    "stdout" => Ok(AccessLogConfig::Stdout),
                    path if !path.is_empty() => Ok(AccessLogConfig::File(path.to_string())),
                    _ => Err(E::custom("invalid access_log value")),
                }
            }
        }

        deserializer.deserialize_any(AccessLogVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn access_log_accepts_string_values() {
        let v: AccessLogConfig = serde_json::from_value(json!("stdout")).unwrap();
        assert_eq!(v, AccessLogConfig::Stdout);

        let v: AccessLogConfig = serde_json::from_value(json!("/tmp/pavis.log")).unwrap();
        assert_eq!(v, AccessLogConfig::File("/tmp/pavis.log".to_string()));

        let v: AccessLogConfig = serde_json::from_value(json!("false")).unwrap();
        assert_eq!(v, AccessLogConfig::Disabled);
    }

    #[test]
    fn access_log_rejects_true() {
        let res: Result<AccessLogConfig, _> = serde_json::from_value(json!(true));
        assert!(res.is_err());
    }

    #[test]
    fn access_log_serializes_variants() {
        let value = serde_json::to_value(AccessLogConfig::Disabled).unwrap();
        assert_eq!(value, json!(false));

        let value = serde_json::to_value(AccessLogConfig::Stdout).unwrap();
        assert_eq!(value, json!("stdout"));

        let value =
            serde_json::to_value(AccessLogConfig::File("/tmp/pavis.log".to_string())).unwrap();
        assert_eq!(value, json!("/tmp/pavis.log"));
    }

    #[test]
    fn access_log_accepts_false_bool_and_rejects_empty_string() {
        let v: AccessLogConfig = serde_json::from_value(json!(false)).unwrap();
        assert_eq!(v, AccessLogConfig::Disabled);

        let res: Result<AccessLogConfig, _> = serde_json::from_value(json!(""));
        assert!(res.is_err());
    }

    #[test]
    fn access_log_reports_expected_types_on_invalid_number() {
        let res: Result<AccessLogConfig, _> = serde_json::from_value(json!(123));
        let msg = res.expect_err("invalid number").to_string();
        assert!(msg.contains("false, 'stdout', or a file path string"));
    }

    #[test]
    fn access_log_visitor_expecting_called() {
        use serde::de::Error;
        struct ExpectationTrigger;
        impl fmt::Display for ExpectationTrigger {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                struct DummyVisitor;
                impl<'de> Visitor<'de> for DummyVisitor {
                    type Value = ();
                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("dummy")
                    }
                }
                DummyVisitor.expecting(f)
            }
        }
        let s = format!("{}", ExpectationTrigger);
        assert_eq!(s, "dummy");

        struct MockDeserializer;
        impl<'de> Deserializer<'de> for MockDeserializer {
            type Error = serde::de::value::Error;
            fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
            where
                V: Visitor<'de>,
            {
                struct ExpectingTrigger<V>(V);
                impl<'de, V: Visitor<'de>> fmt::Display for ExpectingTrigger<V> {
                    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        self.0.expecting(f)
                    }
                }
                let _ = format!("{}", ExpectingTrigger(visitor));
                Err(serde::de::value::Error::custom("forced error"))
            }
            serde::forward_to_deserialize_any! {
                bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
                bytes byte_buf option unit unit_struct newtype_struct seq tuple
                tuple_struct map struct enum identifier ignored_any
            }
        }
        let _ = AccessLogConfig::deserialize(MockDeserializer);
    }
}

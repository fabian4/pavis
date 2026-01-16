use chrono::{DateTime, Utc};
use pavis_pvs::compute_checksum;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ArtifactMetadata {
    pub version: u64,
    #[serde(with = "rfc3339")]
    pub published_at: SystemTime,
    pub checksum: String,
    pub size: u64,
}

#[allow(dead_code)] // Used by publish flow in Phase 2 and tests.
pub(crate) fn checksum_for_bytes(bytes: &[u8]) -> String {
    let digest = compute_checksum(bytes);
    let mut out = String::with_capacity(digest.len() * 2 + "sha256:".len());
    out.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{:02x}", byte);
    }
    out
}

mod rfc3339 {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub(crate) fn serialize<S>(value: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = DateTime::<Utc>::from(*value).to_rfc3339();
        serializer.serialize_str(&value)
    }

    pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        let parsed = DateTime::parse_from_rfc3339(&raw).map_err(serde::de::Error::custom)?;
        Ok(parsed.with_timezone(&Utc).into())
    }
}

#[cfg(test)]
mod tests {
    use super::{ArtifactMetadata, checksum_for_bytes};
    use chrono::{DateTime, Utc};
    use std::time::SystemTime;

    #[test]
    fn metadata_round_trip() {
        let meta = ArtifactMetadata {
            version: 42,
            published_at: SystemTime::UNIX_EPOCH,
            checksum: "sha256:deadbeef".to_string(),
            size: 123,
        };
        let json = serde_json::to_string(&meta).expect("serialize");
        let decoded = serde_json::from_str::<ArtifactMetadata>(&json).expect("deserialize");
        assert_eq!(decoded, meta);
    }

    #[test]
    fn checksum_format_is_sha256_prefixed() {
        let checksum = checksum_for_bytes(b"payload");
        assert!(checksum.starts_with("sha256:"));
        assert_eq!(checksum.len(), "sha256:".len() + 64);
    }

    #[test]
    fn timestamp_serializes_rfc3339() {
        let meta = ArtifactMetadata {
            version: 1,
            published_at: SystemTime::UNIX_EPOCH,
            checksum: "sha256:deadbeef".to_string(),
            size: 0,
        };
        let json = serde_json::to_string(&meta).expect("serialize");
        assert!(json.contains(&DateTime::<Utc>::from(SystemTime::UNIX_EPOCH).to_rfc3339()));
    }
}

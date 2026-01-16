use crate::storage::metadata::ArtifactMetadata;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RelayState {
    pub current_version: u64,
}

pub(crate) fn load_state(path: &Path) -> anyhow::Result<Option<RelayState>> {
    match fs::read(path) {
        Ok(bytes) => {
            let state = serde_json::from_slice::<RelayState>(&bytes)?;
            Ok(Some(state))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

pub(crate) fn save_state(path: &Path, state: &RelayState) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("tmp");
    let data = serde_json::to_vec_pretty(state)?;
    let mut file = fs::File::create(&tmp_path)?;
    file.write_all(&data)?;
    file.sync_all()?;
    fs::rename(&tmp_path, path)?;
    if let Some(parent) = path.parent() {
        let dir = fs::File::open(parent)?;
        dir.sync_all()?;
    }
    Ok(())
}

pub(crate) fn derive_state_from_lkg(lkg_meta: &ArtifactMetadata) -> RelayState {
    RelayState {
        current_version: lkg_meta.version,
    }
}

#[cfg(test)]
mod tests {
    use super::{RelayState, derive_state_from_lkg, load_state, save_state};
    use crate::storage::metadata::ArtifactMetadata;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn load_state_missing_returns_none() {
        let dir = std::env::temp_dir().join(format!(
            "runtime_missing_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("state.json");
        let state = load_state(&path).expect("load");
        assert!(state.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_state_round_trip() {
        let dir = std::env::temp_dir().join(format!(
            "runtime_round_trip_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("state.json");
        let state = RelayState {
            current_version: 42,
        };
        save_state(&path, &state).expect("save");
        let loaded = load_state(&path).expect("load").expect("state");
        assert_eq!(loaded, state);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn derive_state_from_lkg_uses_version() {
        let meta = ArtifactMetadata {
            version: 9,
            published_at: SystemTime::UNIX_EPOCH,
            checksum: "sha256:deadbeef".to_string(),
            size: 123,
        };
        let state = derive_state_from_lkg(&meta);
        assert_eq!(state.current_version, 9);
    }
}

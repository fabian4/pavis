use crate::storage::metadata::ArtifactMetadata;
use crate::storage::validated_path::ValidatedStorageRoot;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const STATE_FILE: &str = "state.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RelayState {
    pub current_version: u64,
}

pub(crate) fn load_state(
    storage_root: &ValidatedStorageRoot,
) -> anyhow::Result<Option<RelayState>> {
    let path = state_file_path(storage_root)?;
    let bytes = match read_if_present(storage_root, &path)? {
        Some(bytes) => bytes,
        None => return Ok(None),
    };
    let state = serde_json::from_slice::<RelayState>(&bytes)?;
    Ok(Some(state))
}

pub(crate) fn save_state(
    storage_root: &ValidatedStorageRoot,
    state: &RelayState,
) -> anyhow::Result<()> {
    let path = state_file_path(storage_root)?;
    let data = serde_json::to_vec_pretty(state)?;
    write_atomic(storage_root, &path, &data)?;
    if let Some(parent) = path.parent() {
        let dir = fs::File::open(ensure_existing_path(storage_root, parent)?)?;
        dir.sync_all()?;
    }
    Ok(())
}

pub(crate) fn derive_state_from_lkg(lkg_meta: &ArtifactMetadata) -> RelayState {
    RelayState {
        current_version: lkg_meta.version,
    }
}

fn ensure_existing_path(
    storage_root: &ValidatedStorageRoot,
    path: &Path,
) -> anyhow::Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", path.display()))?;
    if !canonical.starts_with(storage_root.as_path()) {
        anyhow::bail!(
            "state path escaped root: {} not under {}",
            canonical.display(),
            storage_root.as_path().display()
        );
    }
    Ok(canonical)
}

fn state_file_path(storage_root: &ValidatedStorageRoot) -> anyhow::Result<PathBuf> {
    let root = ensure_existing_path(storage_root, storage_root.as_path())?;
    let candidate = storage_root.state_json_path();
    let name = candidate
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("invalid state path: {}", candidate.display()))?;
    if name != STATE_FILE {
        anyhow::bail!("unexpected state file name: {}", candidate.display());
    }
    Ok(root.join(name))
}

fn read_if_present(
    storage_root: &ValidatedStorageRoot,
    path: &Path,
) -> anyhow::Result<Option<Vec<u8>>> {
    let canonical = match ensure_existing_path(storage_root, path) {
        Ok(path) => path,
        Err(err) => {
            if let Some(io) = err.downcast_ref::<std::io::Error>()
                && io.kind() == std::io::ErrorKind::NotFound
            {
                return Ok(None);
            }
            return Err(err);
        }
    };
    match fs::read(canonical) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn write_atomic(
    storage_root: &ValidatedStorageRoot,
    path: &Path,
    contents: &[u8],
) -> anyhow::Result<()> {
    ensure_parent_within_root(storage_root, path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("tmp");
    ensure_parent_within_root(storage_root, &tmp_path)?;
    let mut file = fs::File::create(&tmp_path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn ensure_parent_within_root(
    storage_root: &ValidatedStorageRoot,
    path: &Path,
) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path has no parent: {}", path.display()))?;
    if !parent.starts_with(storage_root.as_path()) {
        anyhow::bail!(
            "state parent escaped root: {} not under {}",
            parent.display(),
            storage_root.as_path().display(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{RelayState, derive_state_from_lkg, load_state, save_state};
    use crate::storage::metadata::ArtifactMetadata;
    use crate::storage::validated_path::ValidatedStorageRoot;
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
        let storage_root = ValidatedStorageRoot::new(dir.clone()).expect("validated storage root");
        let state = load_state(&storage_root).expect("load");
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
        let storage_root = ValidatedStorageRoot::new(dir.clone()).expect("validated storage root");
        let state = RelayState {
            current_version: 42,
        };
        save_state(&storage_root, &state).expect("save");
        let loaded = load_state(&storage_root).expect("load").expect("state");
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

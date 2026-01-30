use crate::storage::history::{
    history_artifact_path, history_metadata_path, list_history_versions,
};
use crate::storage::metadata::ArtifactMetadata;
use crate::storage::validated_path::ValidatedStorageRoot;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) fn lkg_artifact_path(base: &ValidatedStorageRoot) -> PathBuf {
    base.join("lkg").join("config.pvs")
}

pub(crate) fn lkg_metadata_path(base: &ValidatedStorageRoot) -> PathBuf {
    base.join("lkg").join("meta.json")
}

pub(crate) fn promote_to_lkg(
    base: &ValidatedStorageRoot,
    artifact: &[u8],
    meta: &ArtifactMetadata,
) -> anyhow::Result<()> {
    let dir = base.join("lkg");
    fs::create_dir_all(&dir)?;

    let artifact_path = lkg_artifact_path(base);
    let metadata_path = lkg_metadata_path(base);
    write_atomic(&artifact_path, artifact)?;
    let meta_json = serde_json::to_vec_pretty(meta)?;
    write_atomic(&metadata_path, &meta_json)?;
    fsync_dir(&dir)?;

    Ok(())
}

pub(crate) fn load_lkg(
    base: &ValidatedStorageRoot,
) -> anyhow::Result<Option<(Vec<u8>, ArtifactMetadata)>> {
    let artifact_path = lkg_artifact_path(base);
    let metadata_path = lkg_metadata_path(base);

    let artifact_exists = artifact_path.exists();
    let metadata_exists = metadata_path.exists();

    match (artifact_exists, metadata_exists) {
        (false, false) => Ok(None),
        (true, true) => {
            let bytes = fs::read(&artifact_path)?;
            let meta_bytes = fs::read(&metadata_path)?;
            let meta = serde_json::from_slice::<ArtifactMetadata>(&meta_bytes)?;
            Ok(Some((bytes, meta)))
        }
        (true, false) => Err(anyhow::anyhow!("LKG artifact exists without metadata")),
        (false, true) => Err(anyhow::anyhow!("LKG metadata exists without artifact")),
    }
}

pub(crate) fn load_lkg_metadata(
    base: &ValidatedStorageRoot,
) -> anyhow::Result<Option<ArtifactMetadata>> {
    let metadata_path = lkg_metadata_path(base);
    if !metadata_path.exists() {
        return Ok(None);
    }
    let meta_bytes = fs::read(&metadata_path)?;
    let meta = serde_json::from_slice::<ArtifactMetadata>(&meta_bytes)?;
    Ok(Some(meta))
}

pub(crate) fn repair_lkg(base: &ValidatedStorageRoot) -> anyhow::Result<()> {
    let artifact_path = lkg_artifact_path(base);
    let metadata_path = lkg_metadata_path(base);

    let artifact_exists = artifact_path.exists();
    let metadata_exists = metadata_path.exists();

    if !artifact_exists && !metadata_exists {
        return Ok(());
    }

    if metadata_exists && !artifact_exists {
        let meta_bytes = fs::read(&metadata_path)?;
        let meta = serde_json::from_slice::<ArtifactMetadata>(&meta_bytes)?;
        recover_lkg_from_history(base, meta.version)?;
        return Ok(());
    }

    if artifact_exists && !metadata_exists {
        if let Some(version) = max_history_version(base)?
            && recover_lkg_from_history(base, version).is_ok()
        {
            return Ok(());
        }
        let _ = fs::remove_file(&artifact_path);
        return Ok(());
    }

    Ok(())
}

fn max_history_version(base: &ValidatedStorageRoot) -> anyhow::Result<Option<u64>> {
    let versions = list_history_versions(base)?;
    Ok(versions.into_iter().max())
}

fn recover_lkg_from_history(base: &ValidatedStorageRoot, version: u64) -> anyhow::Result<()> {
    let artifact_path = history_artifact_path(base, version);
    let metadata_path = history_metadata_path(base, version);

    if !artifact_path.exists() || !metadata_path.exists() {
        return Err(anyhow::anyhow!(
            "history entry missing for version {}",
            version
        ));
    }

    let bytes = fs::read(&artifact_path)?;
    let meta_bytes = fs::read(&metadata_path)?;
    let meta = serde_json::from_slice::<ArtifactMetadata>(&meta_bytes)?;
    promote_to_lkg(base, &bytes, &meta)
}

fn write_atomic(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("tmp");
    let mut file = fs::File::create(&tmp_path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn fsync_dir(path: &Path) -> anyhow::Result<()> {
    let dir = fs::File::open(path)?;
    dir.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{lkg_artifact_path, lkg_metadata_path, load_lkg, promote_to_lkg, repair_lkg};
    use crate::storage::history::append_to_history;
    use crate::storage::metadata::ArtifactMetadata;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::super::validated_path::ValidatedStorageRoot;

    fn temp_dir(name: &str) -> ValidatedStorageRoot {
        let dir = std::env::temp_dir().join(format!(
            "relay_lkg_{name}_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        ValidatedStorageRoot::new(dir).expect("validated path")
    }

    #[test]
    fn promote_to_lkg_writes_files() {
        let dir = temp_dir("promote");
        let meta = ArtifactMetadata {
            version: 1,
            published_at: SystemTime::UNIX_EPOCH,
            checksum: "sha256:deadbeef".to_string(),
            size: 3,
        };
        promote_to_lkg(&dir, b"pvs", &meta).expect("promote");
        assert!(lkg_artifact_path(&dir).exists());
        assert!(lkg_metadata_path(&dir).exists());
        let _ = std::fs::remove_dir_all(dir.as_path());
    }

    #[test]
    fn load_lkg_missing_returns_none() {
        let dir = temp_dir("missing");
        let lkg = load_lkg(&dir).expect("load");
        assert!(lkg.is_none());
        let _ = std::fs::remove_dir_all(dir.as_path());
    }

    #[test]
    fn load_lkg_missing_metadata_errors() {
        let dir = temp_dir("missing_meta");
        std::fs::create_dir_all(lkg_artifact_path(&dir).parent().unwrap()).unwrap();
        std::fs::write(lkg_artifact_path(&dir), b"pvs").unwrap();
        let err = load_lkg(&dir).expect_err("error");
        assert!(err.to_string().contains("metadata"));
        let _ = std::fs::remove_dir_all(dir.as_path());
    }

    #[test]
    fn load_lkg_missing_artifact_errors() {
        let dir = temp_dir("missing_artifact");
        let meta = ArtifactMetadata {
            version: 1,
            published_at: SystemTime::UNIX_EPOCH,
            checksum: "sha256:deadbeef".to_string(),
            size: 3,
        };
        std::fs::create_dir_all(lkg_metadata_path(&dir).parent().unwrap()).unwrap();
        std::fs::write(lkg_metadata_path(&dir), serde_json::to_vec(&meta).unwrap()).unwrap();
        let err = load_lkg(&dir).expect_err("error");
        assert!(err.to_string().contains("artifact"));
        let _ = std::fs::remove_dir_all(dir.as_path());
    }

    #[test]
    fn repair_lkg_deletes_orphaned_artifact() {
        let dir = temp_dir("repair_orphan");
        std::fs::create_dir_all(lkg_artifact_path(&dir).parent().unwrap()).unwrap();
        std::fs::write(lkg_artifact_path(&dir), b"pvs").unwrap();
        repair_lkg(&dir).expect("repair");
        assert!(!lkg_artifact_path(&dir).exists());
        let _ = std::fs::remove_dir_all(dir.as_path());
    }

    #[test]
    fn repair_lkg_recovers_from_history() {
        let dir = temp_dir("repair_history");
        let meta = ArtifactMetadata {
            version: 7,
            published_at: SystemTime::UNIX_EPOCH,
            checksum: "sha256:deadbeef".to_string(),
            size: 3,
        };
        append_to_history(&dir, 7, b"pvs", &meta).expect("history");
        std::fs::create_dir_all(lkg_metadata_path(&dir).parent().unwrap()).unwrap();
        std::fs::write(lkg_metadata_path(&dir), serde_json::to_vec(&meta).unwrap()).unwrap();
        repair_lkg(&dir).expect("repair");
        assert!(lkg_artifact_path(&dir).exists());
        let _ = std::fs::remove_dir_all(dir.as_path());
    }
}

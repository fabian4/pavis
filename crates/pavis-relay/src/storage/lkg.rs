use crate::storage::history::{
    history_artifact_path, history_metadata_path, list_history_versions,
};
use crate::storage::metadata::ArtifactMetadata;
use crate::storage::validated_path::ValidatedStorageRoot;
use anyhow::Context;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const LKG_DIR: &str = "lkg";
const LKG_ARTIFACT_FILE: &str = "config.pvs";
const LKG_METADATA_FILE: &str = "meta.json";

#[derive(Clone, Copy, PartialEq, Eq)]
enum LkgFile {
    Artifact,
    Metadata,
}

pub(crate) fn lkg_artifact_path(base: &ValidatedStorageRoot) -> PathBuf {
    base.join(LKG_DIR).join(LKG_ARTIFACT_FILE)
}

pub(crate) fn lkg_metadata_path(base: &ValidatedStorageRoot) -> PathBuf {
    base.join(LKG_DIR).join(LKG_METADATA_FILE)
}

pub(crate) fn promote_to_lkg(
    base: &ValidatedStorageRoot,
    artifact: &[u8],
    meta: &ArtifactMetadata,
) -> anyhow::Result<()> {
    let dir = ensure_lkg_dir(base)?;
    write_atomic_in_dir(base, &dir, LKG_ARTIFACT_FILE, artifact)?;
    let meta_json = serde_json::to_vec_pretty(meta)?;
    write_atomic_in_dir(base, &dir, LKG_METADATA_FILE, &meta_json)?;
    fsync_dir(&dir)?;

    Ok(())
}

pub(crate) fn load_lkg(
    base: &ValidatedStorageRoot,
) -> anyhow::Result<Option<(Vec<u8>, ArtifactMetadata)>> {
    let artifact_bytes = read_lkg_file_if_present(base, LkgFile::Artifact)?;
    let metadata_bytes = read_lkg_file_if_present(base, LkgFile::Metadata)?;

    match (artifact_bytes, metadata_bytes) {
        (None, None) => Ok(None),
        (Some(bytes), Some(meta_bytes)) => {
            let meta = serde_json::from_slice::<ArtifactMetadata>(&meta_bytes)?;
            Ok(Some((bytes, meta)))
        }
        (Some(_), None) => Err(anyhow::anyhow!("LKG artifact exists without metadata")),
        (None, Some(_)) => Err(anyhow::anyhow!("LKG metadata exists without artifact")),
    }
}

pub(crate) fn load_lkg_metadata(
    base: &ValidatedStorageRoot,
) -> anyhow::Result<Option<ArtifactMetadata>> {
    let meta_bytes = match read_lkg_file_if_present(base, LkgFile::Metadata)? {
        Some(bytes) => bytes,
        None => return Ok(None),
    };
    let meta = serde_json::from_slice::<ArtifactMetadata>(&meta_bytes)?;
    Ok(Some(meta))
}

pub(crate) fn repair_lkg(base: &ValidatedStorageRoot) -> anyhow::Result<()> {
    let artifact_bytes = read_lkg_file_if_present(base, LkgFile::Artifact)?;
    let metadata_bytes = read_lkg_file_if_present(base, LkgFile::Metadata)?;

    match (artifact_bytes, metadata_bytes) {
        (None, None) => Ok(()),
        (None, Some(meta_bytes)) => {
            let meta = serde_json::from_slice::<ArtifactMetadata>(&meta_bytes)?;
            recover_lkg_from_history(base, meta.version)
        }
        (Some(_), None) => {
            if let Some(version) = max_history_version(base)?
                && recover_lkg_from_history(base, version).is_ok()
            {
                return Ok(());
            }
            if let Some(path) = lkg_file_path_if_exists(base, LkgFile::Artifact)? {
                let _ = fs::remove_file(path);
            }
            Ok(())
        }
        (Some(_), Some(_)) => Ok(()),
    }
}

fn max_history_version(base: &ValidatedStorageRoot) -> anyhow::Result<Option<u64>> {
    let versions = list_history_versions(base)?;
    Ok(versions.into_iter().max())
}

fn recover_lkg_from_history(base: &ValidatedStorageRoot, version: u64) -> anyhow::Result<()> {
    let artifact_path = history_artifact_path(base, version);
    let metadata_path = history_metadata_path(base, version);
    let bytes = read_if_present(base, &artifact_path)?
        .ok_or_else(|| anyhow::anyhow!("history entry missing artifact for version {}", version))?;
    let meta_bytes = read_if_present(base, &metadata_path)?
        .ok_or_else(|| anyhow::anyhow!("history entry missing metadata for version {}", version))?;
    let meta = serde_json::from_slice::<ArtifactMetadata>(&meta_bytes)?;
    promote_to_lkg(base, &bytes, &meta)
}

fn lkg_file_path_if_exists(
    base: &ValidatedStorageRoot,
    file: LkgFile,
) -> anyhow::Result<Option<PathBuf>> {
    let dir = match ensure_existing_path(base, &base.join(LKG_DIR)) {
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
    let candidate = match file {
        LkgFile::Artifact => lkg_artifact_path(base),
        LkgFile::Metadata => lkg_metadata_path(base),
    };
    let name = candidate
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("invalid LKG file name: {}", candidate.display()))?;
    Ok(Some(dir.join(name)))
}

fn read_lkg_file_if_present(
    base: &ValidatedStorageRoot,
    file: LkgFile,
) -> anyhow::Result<Option<Vec<u8>>> {
    let path = match lkg_file_path_if_exists(base, file)? {
        Some(path) => path,
        None => return Ok(None),
    };
    read_if_present(base, &path)
}

fn ensure_lkg_dir(base: &ValidatedStorageRoot) -> anyhow::Result<PathBuf> {
    let root = ensure_existing_path(base, base.as_path())?;
    let dir = root.join(LKG_DIR);
    match fs::create_dir(&dir) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(err) => return Err(err.into()),
    }
    ensure_existing_path(base, &dir)
}

fn read_if_present(base: &ValidatedStorageRoot, path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    let canonical = match ensure_existing_path(base, path) {
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

fn write_atomic_in_dir(
    base: &ValidatedStorageRoot,
    dir: &Path,
    file_name: &str,
    contents: &[u8],
) -> anyhow::Result<()> {
    if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
        anyhow::bail!("invalid file name: {file_name}");
    }
    let path = dir.join(file_name);
    if !path.starts_with(base.as_path()) {
        anyhow::bail!("path escaped storage root: {}", path.display());
    }
    let tmp_path = dir.join(format!("{file_name}.tmp"));
    if !tmp_path.starts_with(base.as_path()) {
        anyhow::bail!("tmp path escaped storage root: {}", tmp_path.display());
    }
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

fn ensure_existing_path(base: &ValidatedStorageRoot, path: &Path) -> anyhow::Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", path.display()))?;
    if !canonical.starts_with(base.as_path()) {
        anyhow::bail!(
            "storage path escaped root: {} not under {}",
            canonical.display(),
            base.as_path().display()
        );
    }
    Ok(canonical)
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

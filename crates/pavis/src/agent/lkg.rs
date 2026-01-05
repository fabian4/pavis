use pavis_core::ValidatedRuntimeConfig;
use std::path::{Path, PathBuf};

pub fn load_lkg_config(path: &Path) -> anyhow::Result<(ValidatedRuntimeConfig, u64)> {
    let config = pavis_pvs::load(path)?;
    let validated = unsafe { ValidatedRuntimeConfig::from_trusted(config) };
    let version = lkg_version(path)?;
    Ok((validated, version))
}

pub fn lkg_version(lkg_path: &Path) -> anyhow::Result<u64> {
    let path = version_path_for(lkg_path);
    match read_lkg_version(&path) {
        Some(version) => Ok(version),
        None => {
            tracing::warn!(path = %path.display(), "missing LKG version metadata, defaulting to 0");
            Ok(0)
        }
    }
}

pub(crate) fn version_path_for(lkg_path: &Path) -> PathBuf {
    lkg_path.with_extension("pvs.version")
}

pub(crate) fn tmp_path_for(lkg_path: &Path) -> PathBuf {
    let mut tmp = lkg_path.to_path_buf();
    tmp.set_extension("tmp.pvs");
    tmp
}

pub(crate) async fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, bytes).await?;
    Ok(())
}

pub(crate) async fn write_version(path: &Path, version: u64) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, version.to_string()).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

pub(crate) fn read_lkg_version(path: &Path) -> Option<u64> {
    let value = std::fs::read_to_string(path).ok()?;
    value.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{
        lkg_version, read_lkg_version, tmp_path_for, version_path_for, write_atomic, write_version,
    };
    use std::path::PathBuf;

    #[tokio::test]
    async fn version_write_round_trip() {
        let dir = std::env::temp_dir().join("pavis_version_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create dir");
        let lkg = dir.join("config.pvs");
        let version_path = version_path_for(&lkg);
        write_version(&version_path, 12).await.expect("write");
        assert_eq!(read_lkg_version(&version_path), Some(12));
        assert_eq!(lkg_version(&lkg).expect("version"), 12);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tmp_path_suffixes() {
        let lkg = PathBuf::from("/tmp/config.pvs");
        let tmp = tmp_path_for(&lkg);
        assert!(tmp.ends_with("config.tmp.pvs"));
    }

    #[tokio::test]
    async fn atomic_write_writes_bytes() {
        let dir = std::env::temp_dir().join("pavis_atomic_write_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("config.tmp.pvs");
        write_atomic(&path, b"test-bytes").await.expect("write");
        let contents = std::fs::read(&path).expect("read");
        assert_eq!(contents, b"test-bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

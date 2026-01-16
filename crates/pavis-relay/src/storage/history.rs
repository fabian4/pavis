use crate::storage::metadata::ArtifactMetadata;
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const VERSION_WIDTH: usize = 10;

pub(crate) fn history_artifact_path(base: &Path, version: u64) -> PathBuf {
    base.join("history")
        .join(format!("{version:0width$}.pvs", width = VERSION_WIDTH))
}

pub(crate) fn history_metadata_path(base: &Path, version: u64) -> PathBuf {
    base.join("history").join(format!(
        "{version:0width$}.meta.json",
        width = VERSION_WIDTH
    ))
}

#[allow(dead_code)] // Used by publish flow in Phase 2 and tests.
pub(crate) fn append_to_history(
    base: &Path,
    version: u64,
    artifact: &[u8],
    meta: &ArtifactMetadata,
) -> anyhow::Result<()> {
    let dir = base.join("history");
    fs::create_dir_all(&dir)?;
    let artifact_path = history_artifact_path(base, version);
    let metadata_path = history_metadata_path(base, version);

    write_atomic(&artifact_path, artifact)?;
    let meta_json = serde_json::to_vec_pretty(meta)?;
    write_atomic(&metadata_path, &meta_json)?;
    fsync_dir(&dir)?;

    Ok(())
}

pub(crate) fn list_history_versions(base: &Path) -> anyhow::Result<Vec<u64>> {
    let (artifact_versions, meta_versions) = scan_history_sets(base)?;
    let mut versions: Vec<u64> = artifact_versions
        .intersection(&meta_versions)
        .copied()
        .collect();
    versions.sort_unstable();
    Ok(versions)
}

pub(crate) fn find_orphaned_versions(
    base: &Path,
    current_version: u64,
) -> anyhow::Result<Vec<u64>> {
    let versions = list_history_versions(base)?;
    Ok(versions
        .into_iter()
        .filter(|version| *version > current_version)
        .collect())
}

pub(crate) fn find_corrupt_versions(base: &Path) -> anyhow::Result<Vec<u64>> {
    let (artifact_versions, meta_versions) = scan_history_sets(base)?;
    let mut corrupt: Vec<u64> = artifact_versions
        .symmetric_difference(&meta_versions)
        .copied()
        .collect();
    corrupt.sort_unstable();
    Ok(corrupt)
}

fn scan_history_sets(base: &Path) -> anyhow::Result<(HashSet<u64>, HashSet<u64>)> {
    let dir = base.join("history");
    if !dir.exists() {
        return Ok((HashSet::new(), HashSet::new()));
    }

    let mut artifact_versions = HashSet::new();
    let mut meta_versions = HashSet::new();

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if let Some(version) = parse_version(&name, ".pvs") {
            artifact_versions.insert(version);
        } else if let Some(version) = parse_version(&name, ".meta.json") {
            meta_versions.insert(version);
        }
    }

    Ok((artifact_versions, meta_versions))
}

fn parse_version(name: &str, suffix: &str) -> Option<u64> {
    if !name.ends_with(suffix) {
        return None;
    }
    let stem = name.strip_suffix(suffix)?;
    if stem.len() != VERSION_WIDTH || !stem.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    stem.parse::<u64>().ok()
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
    use super::{
        append_to_history, find_orphaned_versions, history_artifact_path, history_metadata_path,
        list_history_versions,
    };
    use crate::storage::metadata::ArtifactMetadata;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "relay_history_{name}_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[test]
    fn append_to_history_creates_expected_paths() {
        let dir = temp_dir("append");
        let meta = ArtifactMetadata {
            version: 1,
            published_at: SystemTime::UNIX_EPOCH,
            checksum: "sha256:deadbeef".to_string(),
            size: 3,
        };
        append_to_history(&dir, 1, b"pvs", &meta).expect("append");
        assert!(history_artifact_path(&dir, 1).exists());
        assert!(history_metadata_path(&dir, 1).exists());
        assert!(
            !history_artifact_path(&dir, 1)
                .with_extension("tmp")
                .exists()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_history_versions_parses_and_sorts() {
        let dir = temp_dir("list");
        let meta = ArtifactMetadata {
            version: 2,
            published_at: SystemTime::UNIX_EPOCH,
            checksum: "sha256:deadbeef".to_string(),
            size: 1,
        };
        append_to_history(&dir, 2, b"a", &meta).expect("append");
        let meta3 = ArtifactMetadata { version: 3, ..meta };
        append_to_history(&dir, 3, b"b", &meta3).expect("append");
        let versions = list_history_versions(&dir).expect("list");
        assert_eq!(versions, vec![2, 3]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_orphaned_versions_filters_current() {
        let dir = temp_dir("orphans");
        let meta = ArtifactMetadata {
            version: 5,
            published_at: SystemTime::UNIX_EPOCH,
            checksum: "sha256:deadbeef".to_string(),
            size: 1,
        };
        append_to_history(&dir, 5, b"a", &meta).expect("append");
        let meta6 = ArtifactMetadata { version: 6, ..meta };
        append_to_history(&dir, 6, b"b", &meta6).expect("append");
        let orphans = find_orphaned_versions(&dir, 5).expect("orphans");
        assert_eq!(orphans, vec![6]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

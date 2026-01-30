//! Validated storage root paths to prevent path traversal attacks.
//!
//! This module provides a newtype wrapper that ensures storage root paths are:
//! - Canonicalized (symlinks resolved, normalized)
//! - Absolute (no relative path components)
//! - Validated at construction time (following Zero-Option philosophy)
//!
//! # Security
//!
//! By validating paths at construction, we make path traversal attacks structurally
//! unrepresentable. The type system guarantees that any `ValidatedStorageRoot`
//! has already passed validation.

use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during storage path validation.
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Storage root path cannot be empty")]
    EmptyPath,

    #[error("Failed to canonicalize path: {0}")]
    CanonicalizationFailed(#[source] std::io::Error),

    #[error("Failed to create parent directories: {0}")]
    DirectoryCreationFailed(#[source] std::io::Error),

    #[error("Path is not absolute after canonicalization: {0}")]
    NotAbsolute(PathBuf),
}

/// A validated storage root path that has been canonicalized and verified.
///
/// This type guarantees:
/// - The path is not empty
/// - The path is absolute
/// - Symlinks have been resolved
/// - Parent directories exist
///
/// # Security
///
/// This prevents path traversal attacks by ensuring the storage root is always
/// a known, validated location. Internal path components (e.g., "lkg", "history")
/// are trusted constants appended to this validated root.
#[derive(Debug, Clone)]
pub struct ValidatedStorageRoot {
    canonical: PathBuf,
}

impl ValidatedStorageRoot {
    /// Validate and canonicalize a storage root path.
    ///
    /// # Validation Steps
    ///
    /// 1. Reject empty paths
    /// 2. Convert to absolute path (resolving against current directory if relative)
    /// 3. Create the directory (and all parents) if it doesn't exist
    /// 4. Canonicalize to resolve symlinks and normalize the path
    /// 5. Verify the resulting path is absolute
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path is empty
    /// - Directories cannot be created
    /// - The path cannot be canonicalized
    /// - The canonicalized path is not absolute
    pub fn new(path: PathBuf) -> Result<Self, ValidationError> {
        // 1. Check path is not empty
        if path.as_os_str().is_empty() {
            return Err(ValidationError::EmptyPath);
        }

        // 2. Convert to absolute path if relative
        let absolute = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .map_err(ValidationError::CanonicalizationFailed)?
                .join(path)
        };

        // 3. Create the directory (and all parents) if it doesn't exist
        std::fs::create_dir_all(&absolute).map_err(ValidationError::DirectoryCreationFailed)?;

        // 4. Canonicalize to resolve symlinks and normalize
        let canonical = absolute
            .canonicalize()
            .map_err(ValidationError::CanonicalizationFailed)?;

        // 5. Verify resulting path is absolute
        if !canonical.is_absolute() {
            return Err(ValidationError::NotAbsolute(canonical));
        }

        Ok(Self { canonical })
    }

    /// Get the validated path as a `&Path`.
    ///
    /// This is safe because the path has been validated at construction time.
    pub fn as_path(&self) -> &Path {
        &self.canonical
    }

    /// Join a relative path component to this validated root.
    ///
    /// # Security
    ///
    /// This method is safe because:
    /// - The base path is already validated
    /// - Internal components (like "lkg", "history") are trusted constants
    ///
    /// For additional safety, callers should avoid joining user-controlled strings.
    pub fn join<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        self.canonical.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn setup_test_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "validated_path_{label}_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_empty_path() {
        let result = ValidatedStorageRoot::new(PathBuf::from(""));
        assert!(matches!(result, Err(ValidationError::EmptyPath)));
    }

    #[test]
    fn accepts_absolute_path() {
        let path = setup_test_dir("abs");
        let result = ValidatedStorageRoot::new(path.clone());
        assert!(result.is_ok());
        let validated = result.unwrap();
        assert!(validated.as_path().is_absolute());
        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn accepts_relative_path() {
        let path = setup_test_dir("rel");
        // Create a subdirectory
        let subdir = path.join("test_subdir");
        fs::create_dir_all(&subdir).unwrap();

        // Change to temp directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&path).unwrap();

        let result = ValidatedStorageRoot::new(PathBuf::from("test_subdir"));
        assert!(result.is_ok());
        let validated = result.unwrap();
        assert!(validated.as_path().is_absolute());

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn canonicalizes_symlinks() {
        let path = setup_test_dir("symlink");
        let target = path.join("target");
        let link = path.join("link");

        fs::create_dir_all(&target).unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &link).unwrap();
            let result = ValidatedStorageRoot::new(link);
            assert!(result.is_ok());
            let validated = result.unwrap();
            // Canonicalized path should resolve to target
            assert!(validated.as_path().ends_with("target"));
        }
        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn creates_parent_directories() {
        let path = setup_test_dir("parent");
        let nested = path.join("a").join("b").join("c");

        let result = ValidatedStorageRoot::new(nested.clone());
        assert!(result.is_ok());

        // Parent directories should exist
        assert!(nested.parent().unwrap().exists());
        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn join_preserves_safety() {
        let path = setup_test_dir("join");
        let validated = ValidatedStorageRoot::new(path.clone()).unwrap();

        let joined = validated.join("lkg").join("config.pvs");
        assert!(joined.starts_with(validated.as_path()));
        assert!(joined.ends_with("lkg/config.pvs"));
        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn rejects_invalid_path_components() {
        // Use a path that contains null bytes, which is always invalid on Unix/Linux
        // This will fail during directory creation regardless of permissions
        let invalid_path = PathBuf::from("/tmp/invalid\0path");
        let result = ValidatedStorageRoot::new(invalid_path);
        assert!(result.is_err());
    }
}

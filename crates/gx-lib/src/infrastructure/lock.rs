use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::domain::{CommitSha, Lock, LockKey};

pub const LOCK_FILE_NAME: &str = "gx.lock";
pub const LOCK_FILE_VERSION: &str = "1.0";

/// Errors that can occur when working with lock files
#[derive(Debug, Error)]
pub enum LockFileError {
    #[error("failed to read lock file: {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse lock file: {}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },

    #[error("failed to write lock file: {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to serialize lock file to TOML")]
    Serialize(#[source] toml::ser::Error),
}

/// Internal structure for TOML serialization
#[derive(Debug, Deserialize, Serialize)]
struct LockData {
    #[serde(default)]
    version: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    actions: HashMap<String, String>,
}

impl Default for LockData {
    fn default() -> Self {
        Self {
            version: LOCK_FILE_VERSION.to_string(),
            actions: HashMap::new(),
        }
    }
}

fn lock_from_data(data: LockData) -> Lock {
    let actions = data
        .actions
        .into_iter()
        .filter_map(|(k, v)| LockKey::parse(&k).map(|key| (key, CommitSha::from(v))))
        .collect();
    Lock::new(actions)
}

fn lock_to_data(lock: &Lock) -> LockData {
    let actions = lock
        .entries()
        .map(|(k, sha)| (k.to_string(), sha.as_str().to_owned()))
        .collect();
    LockData {
        version: LOCK_FILE_VERSION.to_string(),
        actions,
    }
}

/// File-backed lock store. Reads from and writes to `.github/gx.lock`.
pub struct FileLock {
    path: PathBuf,
}

impl FileLock {
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }
}

impl FileLock {
    /// Save the given `Lock` to this file.
    ///
    /// # Errors
    ///
    /// Returns [`LockFileError::Serialize`] if serialization fails.
    /// Returns [`LockFileError::Write`] if the file cannot be written.
    pub fn save(&self, lock: &Lock) -> Result<(), LockFileError> {
        let data = lock_to_data(lock);
        let content = toml::to_string_pretty(&data).map_err(LockFileError::Serialize)?;
        fs::write(&self.path, content).map_err(|source| LockFileError::Write {
            path: self.path.clone(),
            source,
        })?;
        info!("Lock file updated: {}", self.path.display());
        Ok(())
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Load a lock from a file path. Returns `Lock::default()` if the file does not exist.
/// Transparently migrates old lock file versions.
///
/// # Errors
///
/// Returns [`LockFileError::Read`] if the file cannot be read.
/// Returns [`LockFileError::Parse`] if the TOML is invalid.
pub fn parse_lock(path: &Path) -> Result<Lock, LockFileError> {
    if !path.exists() {
        return Ok(Lock::default());
    }

    let content = fs::read_to_string(path).map_err(|source| LockFileError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let data: LockData = toml::from_str(&content).map_err(|source| LockFileError::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;

    let needs_rewrite = data.version != LOCK_FILE_VERSION;
    let lock = lock_from_data(data);

    if needs_rewrite {
        // Reuse FileLock::save logic — create a temporary store to trigger migration write
        let store = FileLock::new(path);
        store.save(&lock)?;
    }

    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ActionId, CommitSha, LockKey, ResolvedAction, Version};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_resolved(action: &str, version: &str, sha: &str) -> ResolvedAction {
        ResolvedAction::new(
            ActionId::from(action),
            Version::from(version),
            CommitSha::from(sha),
        )
    }

    fn make_key(action: &str, version: &str) -> LockKey {
        LockKey::new(ActionId::from(action), Version::from(version))
    }

    #[test]
    fn test_file_lock_save_and_load_roundtrip() {
        let file = NamedTempFile::new().unwrap();
        let store = FileLock::new(file.path());

        let mut lock = Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "v4",
            "abc123def456789012345678901234567890abcd",
        ));

        store.save(&lock).unwrap();

        let loaded = parse_lock(file.path()).unwrap();
        assert_eq!(
            loaded.get(&make_key("actions/checkout", "v4")),
            Some(&CommitSha::from("abc123def456789012345678901234567890abcd"))
        );
    }

    #[test]
    fn test_file_lock_load_existing_toml() {
        let content = format!(
            r#"version = "{LOCK_FILE_VERSION}"

[actions]
"actions/checkout@v4" = "abc123def456789012345678901234567890abcd"
"#
        );
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let lock = parse_lock(file.path()).unwrap();
        assert_eq!(
            lock.get(&make_key("actions/checkout", "v4")),
            Some(&CommitSha::from("abc123def456789012345678901234567890abcd"))
        );
    }

    #[test]
    fn test_file_lock_migrates_old_version() {
        // Old lock file without version field
        let content = r#"
[actions]
"actions/checkout@v4" = "abc123def456789012345678901234567890abcd"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let _lock = parse_lock(file.path()).unwrap();

        // File should now contain version field
        let written = fs::read_to_string(file.path()).unwrap();
        assert!(written.contains(LOCK_FILE_VERSION));
    }

    #[test]
    fn test_parse_lock_missing_returns_empty() {
        let lock = parse_lock(Path::new("/nonexistent/gx.lock")).unwrap();
        assert!(!lock.has(&make_key("actions/checkout", "v4")));
    }

    #[test]
    fn test_parse_lock_reads_file() {
        let content = format!(
            "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@v4\" = \"abc123\"\n"
        );
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let lock = parse_lock(file.path()).unwrap();
        assert!(lock.has(&make_key("actions/checkout", "v4")));
    }
}

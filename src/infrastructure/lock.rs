use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::domain::{CommitSha, Lock, LockKey};

pub const LOCK_FILE_NAME: &str = "gx.lock";
pub const LOCK_FILE_VERSION: &str = "1.0";

/// Pure I/O trait for loading and saving the lock file.
/// Domain operations (get, set, retain, etc.) live on `Lock` in the domain layer.
pub trait LockStore {
    /// Load the lock from storage, returning a `Lock` domain entity.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    fn load(&self) -> Result<Lock, LockFileError>;

    /// Save the given `Lock` to storage.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    fn save(&self, lock: &Lock) -> Result<(), LockFileError>;

    /// The path this store reads from and writes to.
    fn path(&self) -> &Path;
}

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

impl LockStore for FileLock {
    fn load(&self) -> Result<Lock, LockFileError> {
        if !self.path.exists() {
            return Ok(Lock::default());
        }

        let content = fs::read_to_string(&self.path).map_err(|source| LockFileError::Read {
            path: self.path.clone(),
            source,
        })?;

        let data: LockData = toml::from_str(&content).map_err(|source| LockFileError::Parse {
            path: self.path.clone(),
            source: Box::new(source),
        })?;

        let needs_rewrite = data.version != LOCK_FILE_VERSION;
        let lock = lock_from_data(data);

        // Transparently migrate old lock files to the current format version
        if needs_rewrite {
            self.save(&lock)?;
        }

        Ok(lock)
    }

    fn save(&self, lock: &Lock) -> Result<(), LockFileError> {
        let data = lock_to_data(lock);
        let content = toml::to_string_pretty(&data).map_err(LockFileError::Serialize)?;
        fs::write(&self.path, content).map_err(|source| LockFileError::Write {
            path: self.path.clone(),
            source,
        })?;
        info!("Lock file updated: {}", self.path.display());
        Ok(())
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

/// In-memory lock store that doesn't persist to disk. Used when no `gx.toml` exists.
#[derive(Default)]
pub struct MemoryLock;

impl LockStore for MemoryLock {
    fn load(&self) -> Result<Lock, LockFileError> {
        Ok(Lock::default())
    }

    fn save(&self, _lock: &Lock) -> Result<(), LockFileError> {
        Ok(()) // no-op
    }

    fn path(&self) -> &Path {
        Path::new("in-memory")
    }
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
    fn test_file_lock_load_missing_file_returns_empty() {
        let store = FileLock::new(Path::new("/nonexistent/path/gx.lock"));
        let lock = store.load().unwrap();
        assert!(!lock.has(&make_key("actions/checkout", "v4")));
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

        let loaded = store.load().unwrap();
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

        let store = FileLock::new(file.path());
        let lock = store.load().unwrap();
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

        let store = FileLock::new(file.path());
        let _lock = store.load().unwrap();

        // File should now contain version field
        let written = fs::read_to_string(file.path()).unwrap();
        assert!(written.contains(LOCK_FILE_VERSION));
    }

    #[test]
    fn test_memory_lock_load_returns_empty() {
        let store = MemoryLock;
        let lock = store.load().unwrap();
        assert!(!lock.has(&make_key("actions/checkout", "v4")));
    }

    #[test]
    fn test_memory_lock_save_is_noop() {
        let store = MemoryLock;
        let lock = Lock::default();
        assert!(store.save(&lock).is_ok());
    }
}

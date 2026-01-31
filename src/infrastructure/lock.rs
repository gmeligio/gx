use log::info;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::domain::{ActionId, CommitSha, LockKey, ResolvedAction};

pub const LOCK_FILE_NAME: &str = "gx.lock";
pub const LOCK_FILE_VERSION: &str = "1.0";

/// Trait defining operations on a lock file (action@version → SHA mapping)
pub trait LockStore {
    /// Get the locked commit SHA for a lock key
    fn get(&self, key: &LockKey) -> Option<&CommitSha>;

    /// Set or update a locked action with its commit SHA
    fn set(&mut self, resolved: &ResolvedAction);

    /// Check if lock file has an entry for the given key
    fn has(&self, key: &LockKey) -> bool;

    /// Retain only entries for the given keys, removing all others
    fn retain(&mut self, keys: &[LockKey]);

    /// Build a map of action names to "SHA # version" for workflow updates
    fn build_update_map(&self, keys: &[LockKey]) -> HashMap<ActionId, String>;

    /// Save the lock file only if there were changes
    ///
    /// Required for file-based lock files. It's a no-op for in-memory.
    ///
    /// # Errors
    ///
    /// Returns an error if saving is required but fails.
    fn save(&mut self) -> Result<(), LockFileError>;
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

    #[error(
        "`LockFile.path` not initialized. Use load_or_default or load to create a LockFile with a path."
    )]
    PathNotInitialized,
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

/// Lock file structure that maps action@version to resolved commit SHA
#[derive(Debug, Default)]
pub struct FileLock {
    /// Maps `LockKey` to `CommitSha`
    actions: HashMap<LockKey, CommitSha>,
    path: Option<PathBuf>,
    changed: bool,
}

impl FileLock {
    /// Get the path of the lock file.
    ///
    /// # Errors
    ///
    /// Return `PathNotInitialized` if the path is not initialized.
    pub fn path(&self) -> Result<&Path, LockFileError> {
        self.path
            .as_deref()
            .ok_or(LockFileError::PathNotInitialized)
    }

    /// Load a lock file from the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, LockFileError> {
        let content = fs::read_to_string(path).map_err(|source| LockFileError::Read {
            path: path.to_path_buf(),
            source,
        })?;

        let data: LockData = toml::from_str(&content).map_err(|source| LockFileError::Parse {
            path: path.to_path_buf(),
            source: Box::new(source),
        })?;

        let actions = data
            .actions
            .into_iter()
            .filter_map(|(k, v)| LockKey::parse(&k).map(|key| (key, CommitSha(v))))
            .collect();

        // Mark as changed if version differs, triggering an update on save
        let changed = data.version != LOCK_FILE_VERSION;

        Ok(Self {
            actions,
            path: Some(path.to_path_buf()),
            changed,
        })
    }

    /// Load a lock file from the given path, or return a default if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load_or_default(path: &Path) -> Result<Self, LockFileError> {
        if path.exists() {
            Self::load(path)
        } else {
            Ok(Self {
                path: Some(path.to_path_buf()),
                ..Default::default()
            })
        }
    }

    fn save_to_disk(&self) -> Result<(), LockFileError> {
        let path = self.path()?;

        // Convert to serializable format
        let data = LockData {
            version: LOCK_FILE_VERSION.to_string(),
            actions: self
                .actions
                .iter()
                .map(|(k, v)| (k.to_key_string(), v.0.clone()))
                .collect(),
        };

        let content = toml::to_string_pretty(&data).map_err(LockFileError::Serialize)?;

        fs::write(path, content).map_err(|source| LockFileError::Write {
            path: path.to_path_buf(),
            source,
        })?;

        info!("Lock file updated: {}", path.display());
        Ok(())
    }
}

impl LockStore for FileLock {
    fn get(&self, key: &LockKey) -> Option<&CommitSha> {
        self.actions.get(key)
    }

    fn set(&mut self, resolved: &ResolvedAction) {
        let key = LockKey::from(resolved);
        let existing = self.actions.get(&key);
        if existing != Some(&resolved.sha) {
            self.actions.insert(key, resolved.sha.clone());
            self.changed = true;
        }
    }

    fn has(&self, key: &LockKey) -> bool {
        self.actions.contains_key(key)
    }

    fn retain(&mut self, keys: &[LockKey]) {
        let used_keys: HashSet<&LockKey> = keys.iter().collect();
        let original_len = self.actions.len();
        self.actions.retain(|key, _| used_keys.contains(key));
        if self.actions.len() != original_len {
            self.changed = true;
        }
    }

    fn build_update_map(&self, keys: &[LockKey]) -> HashMap<ActionId, String> {
        let mut update_map = HashMap::new();

        for key in keys {
            if let Some(sha) = self.get(key) {
                // Format as "SHA # version" for the workflow update
                let update_value = format!("{} # {}", sha, key.version);
                update_map.insert(key.id.clone(), update_value);
            } else {
                // Fallback to version if SHA not found in lock file
                update_map.insert(key.id.clone(), key.version.0.clone());
            }
        }

        update_map
    }

    fn save(&mut self) -> Result<(), LockFileError> {
        if self.changed {
            self.save_to_disk()?;
            self.changed = false;
        }
        Ok(())
    }
}

/// In-memory lock file that doesn't persist to disk
#[derive(Debug, Default)]
pub struct MemoryLock {
    actions: HashMap<LockKey, CommitSha>,
}

impl LockStore for MemoryLock {
    fn get(&self, key: &LockKey) -> Option<&CommitSha> {
        self.actions.get(key)
    }

    fn set(&mut self, resolved: &ResolvedAction) {
        let key = LockKey::from(resolved);
        self.actions.insert(key, resolved.sha.clone());
    }

    fn has(&self, key: &LockKey) -> bool {
        self.actions.contains_key(key)
    }

    fn retain(&mut self, keys: &[LockKey]) {
        let used_keys: HashSet<&LockKey> = keys.iter().collect();
        self.actions.retain(|key, _| used_keys.contains(key));
    }

    fn build_update_map(&self, keys: &[LockKey]) -> HashMap<ActionId, String> {
        let mut update_map = HashMap::new();

        for key in keys {
            if let Some(sha) = self.get(key) {
                let update_value = format!("{} # {}", sha, key.version);
                update_map.insert(key.id.clone(), update_value);
            } else {
                update_map.insert(key.id.clone(), key.version.0.clone());
            }
        }

        update_map
    }

    fn save(&mut self) -> Result<(), LockFileError> {
        Ok(()) // no-op for in-memory
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ActionId, Version};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_key(action: &str, version: &str) -> LockKey {
        LockKey::new(ActionId::from(action), Version::from(version))
    }

    fn make_resolved(action: &str, version: &str, sha: &str) -> ResolvedAction {
        ResolvedAction::new(
            ActionId::from(action),
            Version::from(version),
            CommitSha::from(sha),
        )
    }

    #[test]
    fn test_parse_lock_file() {
        let content = r#"
[actions]
"actions/checkout@v4" = "abc123def456"
"actions/setup-node@v3" = "789xyz012"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let lock = FileLock::load(file.path()).unwrap();

        assert_eq!(
            lock.get(&make_key("actions/checkout", "v4")),
            Some(&CommitSha::from("abc123def456"))
        );
        assert_eq!(
            lock.get(&make_key("actions/setup-node", "v3")),
            Some(&CommitSha::from("789xyz012"))
        );
    }

    #[test]
    fn test_empty_lock_file() {
        let content = "[actions]\n";

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let lock = FileLock::load(file.path()).unwrap();
        assert!(lock.actions.is_empty());
    }

    #[test]
    fn test_load_or_default_existing() {
        let content = r#"
[actions]
"actions/checkout@v4" = "abc123"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let lock = FileLock::load_or_default(file.path()).unwrap();
        assert_eq!(
            lock.get(&make_key("actions/checkout", "v4")),
            Some(&CommitSha::from("abc123"))
        );
    }

    #[test]
    fn test_load_or_default_missing() {
        let lock = FileLock::load_or_default(Path::new("/nonexistent/path/gx.lock")).unwrap();
        assert!(lock.actions.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let mut lock = FileLock::default();
        lock.set(&make_resolved("actions/checkout", "v4", "abc123def456"));
        lock.set(&make_resolved("actions/setup-node", "v3", "789xyz012"));

        let file = NamedTempFile::new().unwrap();
        lock.path = Some(file.path().to_path_buf());
        lock.save().unwrap();

        let loaded = FileLock::load(file.path()).unwrap();
        assert_eq!(
            loaded.get(&make_key("actions/checkout", "v4")),
            Some(&CommitSha::from("abc123def456"))
        );
        assert_eq!(
            loaded.get(&make_key("actions/setup-node", "v3")),
            Some(&CommitSha::from("789xyz012"))
        );
    }

    #[test]
    fn test_path_not_initialized_error() {
        let lock = FileLock::default();
        let result = lock.path();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("`LockFile.path` not initialized"));
    }

    #[test]
    fn test_set_and_get() {
        let mut lock = FileLock::default();
        lock.set(&make_resolved("actions/checkout", "v4", "abc123"));

        assert_eq!(
            lock.get(&make_key("actions/checkout", "v4")),
            Some(&CommitSha::from("abc123"))
        );
        assert_eq!(lock.get(&make_key("actions/checkout", "v3")), None);
    }

    #[test]
    fn test_has() {
        let mut lock = FileLock::default();
        lock.set(&make_resolved("actions/checkout", "v4", "abc123"));

        assert!(lock.has(&make_key("actions/checkout", "v4")));
        assert!(!lock.has(&make_key("actions/checkout", "v3")));
        assert!(!lock.has(&make_key("actions/setup-node", "v4")));
    }

    #[test]
    fn test_retain() {
        let mut lock = FileLock::default();
        lock.set(&make_resolved("actions/checkout", "v4", "abc123"));
        lock.set(&make_resolved("actions/setup-node", "v3", "def456"));
        lock.set(&make_resolved("actions/old-action", "v1", "xyz789"));

        let used = vec![
            make_key("actions/checkout", "v4"),
            make_key("actions/setup-node", "v3"),
        ];

        lock.retain(&used);

        assert!(lock.has(&make_key("actions/checkout", "v4")));
        assert!(lock.has(&make_key("actions/setup-node", "v3")));
        assert!(!lock.has(&make_key("actions/old-action", "v1")));
    }

    #[test]
    fn test_update_existing_sha() {
        let mut lock = FileLock::default();
        lock.set(&make_resolved("actions/checkout", "v4", "old_sha"));
        lock.set(&make_resolved("actions/checkout", "v4", "new_sha"));

        assert_eq!(
            lock.get(&make_key("actions/checkout", "v4")),
            Some(&CommitSha::from("new_sha"))
        );
    }

    #[test]
    fn test_build_update_map() {
        let mut lock = FileLock::default();
        lock.set(&make_resolved("actions/checkout", "v4", "abc123def456"));
        lock.set(&make_resolved("actions/setup-node", "v3", "789xyz012"));

        let keys = vec![
            make_key("actions/checkout", "v4"),
            make_key("actions/setup-node", "v3"),
        ];

        let update_map = lock.build_update_map(&keys);

        assert_eq!(
            update_map.get(&ActionId::from("actions/checkout")),
            Some(&"abc123def456 # v4".to_string())
        );
        assert_eq!(
            update_map.get(&ActionId::from("actions/setup-node")),
            Some(&"789xyz012 # v3".to_string())
        );
    }

    #[test]
    fn test_build_update_map_fallback_to_version() {
        let lock = FileLock::default(); // Empty lock file

        let keys = vec![make_key("actions/checkout", "v4")];

        let update_map = lock.build_update_map(&keys);

        // Should fallback to version if SHA not in lock file
        assert_eq!(
            update_map.get(&ActionId::from("actions/checkout")),
            Some(&"v4".to_string())
        );
    }

    #[test]
    fn test_version_deserialization() {
        let content = r#"
version = "1.0"

[actions]
"actions/checkout@v4" = "abc123"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let lock = FileLock::load(file.path()).unwrap();
        assert_eq!(
            lock.get(&make_key("actions/checkout", "v4")),
            Some(&CommitSha::from("abc123"))
        );
    }

    #[test]
    fn test_update_content_to_latest_version() {
        // Old lock files without version should get default version and mark as changed
        let content = r#"
[actions]
"actions/checkout@v4" = "abc123"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let lock = FileLock::load(file.path()).unwrap();
        assert!(
            lock.changed,
            "should be marked as changed when version differs"
        );
    }

    #[test]
    fn test_current_version_not_marked_changed() {
        // Lock files with current version should not be marked as changed
        let content = format!(
            r#"version = "{LOCK_FILE_VERSION}"

[actions]
"actions/checkout@v4" = "abc123"
"#
        );

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let lock = FileLock::load(file.path()).unwrap();
        assert!(
            !lock.changed,
            "should not be marked as changed when version matches"
        );
    }
}

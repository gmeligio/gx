use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const LOCK_FILE_NAME: &str = "gx.lock";

/// Trait defining operations on a lock file (action@version → SHA mapping)
pub trait Lock {
    /// Get a reference to the actions map
    fn actions(&self) -> &HashMap<String, String>;

    /// Set or update a locked action version with its commit SHA
    fn set(&mut self, action: &str, version: &str, commit_sha: String);

    /// Get the locked commit SHA for an action@version
    fn get(&self, action: &str, version: &str) -> Option<&String>;

    /// Check if lock file has an entry for the given action@version
    fn has(&self, action: &str, version: &str) -> bool;

    /// Remove entries for actions no longer in use
    fn remove_unused(&mut self, used_actions: &HashMap<String, String>);

    /// Build a map of action names to "SHA # version" for workflow updates
    /// Takes versions from the manifest and SHAs from the lock
    fn build_update_map(
        &self,
        manifest_actions: &HashMap<String, String>,
    ) -> HashMap<String, String>;

    /// Save the lock file only if there were changes
    ///
    /// Required for file-based lock files. It's a no-op for in-memory.
    ///
    /// # Errors
    ///
    /// Returns an error if saving is required but fails.
    fn save_if_changed(&mut self) -> Result<(), LockFileError>;
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
    PathNotInitialized(),
}

/// Lock file structure that maps action@version to resolved commit SHA
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct FileLock {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub actions: HashMap<String, String>,
    #[serde(skip)]
    path: Option<PathBuf>,
    #[serde(skip)]
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
            .ok_or_else(LockFileError::PathNotInitialized)
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

        let mut lock: FileLock =
            toml::from_str(&content).map_err(|source| LockFileError::Parse {
                path: path.to_path_buf(),
                source: Box::new(source),
            })?;

        lock.path = Some(path.to_path_buf());
        Ok(lock)
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
            let lock = Self {
                path: Some(path.to_path_buf()),
                ..Default::default()
            };
            Ok(lock)
        }
    }

    /// Save the lock file to disk.
    ///
    /// # Errors
    ///
    /// - Returns `LockFileError::PathNotInitialized` if the path is not initialized,
    /// - Returns `LockFileError::Serialize` if serialization fails
    /// - Returns `LockFileError::Write` if the file cannot be written.
    pub fn save(&self) -> Result<(), LockFileError> {
        let path = self.path()?;
        let content = toml::to_string_pretty(self).map_err(LockFileError::Serialize)?;

        fs::write(path, content).map_err(|source| LockFileError::Write {
            path: path.to_path_buf(),
            source,
        })?;

        info!("Lock file updated: {}", path.display());
        Ok(())
    }
}

impl Lock for FileLock {
    fn actions(&self) -> &HashMap<String, String> {
        &self.actions
    }

    fn set(&mut self, action: &str, version: &str, commit_sha: String) {
        let key = format!("{action}@{version}");
        let existing = self.actions.get(&key);
        if existing != Some(&commit_sha) {
            self.actions.insert(key, commit_sha);
            self.changed = true;
        }
    }

    fn get(&self, action: &str, version: &str) -> Option<&String> {
        let key = format!("{action}@{version}");
        self.actions.get(&key)
    }

    fn has(&self, action: &str, version: &str) -> bool {
        let key = format!("{action}@{version}");
        self.actions.contains_key(&key)
    }

    fn remove_unused(&mut self, used_actions: &HashMap<String, String>) {
        let used_keys: std::collections::HashSet<String> = used_actions
            .iter()
            .map(|(action, version)| format!("{action}@{version}"))
            .collect();

        let original_len = self.actions.len();
        self.actions.retain(|key, _| used_keys.contains(key));
        if self.actions.len() != original_len {
            self.changed = true;
        }
    }

    fn build_update_map(
        &self,
        manifest_actions: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut update_map = HashMap::new();

        for (action, version) in manifest_actions {
            if let Some(sha) = self.get(action, version) {
                // Format as "SHA # version" for the workflow update
                let update_value = format!("{sha} # {version}");
                update_map.insert(action.clone(), update_value);
            } else {
                // Fallback to version if SHA not found in lock file
                update_map.insert(action.clone(), version.clone());
            }
        }

        update_map
    }

    fn save_if_changed(&mut self) -> Result<(), LockFileError> {
        if self.changed { self.save() } else { Ok(()) }
    }
}

/// In-memory lock file that doesn't persist to disk
#[derive(Debug, Default)]
pub struct MemoryLock {
    actions: HashMap<String, String>,
}

impl Lock for MemoryLock {
    fn actions(&self) -> &HashMap<String, String> {
        &self.actions
    }

    fn set(&mut self, action: &str, version: &str, commit_sha: String) {
        let key = format!("{action}@{version}");
        self.actions.insert(key, commit_sha);
    }

    fn get(&self, action: &str, version: &str) -> Option<&String> {
        let key = format!("{action}@{version}");
        self.actions.get(&key)
    }

    fn has(&self, action: &str, version: &str) -> bool {
        let key = format!("{action}@{version}");
        self.actions.contains_key(&key)
    }

    fn remove_unused(&mut self, used_actions: &HashMap<String, String>) {
        let used_keys: std::collections::HashSet<String> = used_actions
            .iter()
            .map(|(action, version)| format!("{action}@{version}"))
            .collect();

        self.actions.retain(|key, _| used_keys.contains(key));
    }

    fn build_update_map(
        &self,
        manifest_actions: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut update_map = HashMap::new();

        for (action, version) in manifest_actions {
            if let Some(sha) = self.get(action, version) {
                let update_value = format!("{sha} # {version}");
                update_map.insert(action.clone(), update_value);
            } else {
                update_map.insert(action.clone(), version.clone());
            }
        }

        update_map
    }

    fn save_if_changed(&mut self) -> Result<(), LockFileError> {
        Ok(()) // no-op for in-memory
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

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
            lock.get("actions/checkout", "v4"),
            Some(&"abc123def456".to_string())
        );
        assert_eq!(
            lock.get("actions/setup-node", "v3"),
            Some(&"789xyz012".to_string())
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
            lock.get("actions/checkout", "v4"),
            Some(&"abc123".to_string())
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
        lock.set("actions/checkout", "v4", "abc123def456".to_string());
        lock.set("actions/setup-node", "v3", "789xyz012".to_string());

        let file = NamedTempFile::new().unwrap();
        lock.path = Some(file.path().to_path_buf());
        lock.save().unwrap();

        let loaded = FileLock::load(file.path()).unwrap();
        assert_eq!(
            loaded.get("actions/checkout", "v4"),
            Some(&"abc123def456".to_string())
        );
        assert_eq!(
            loaded.get("actions/setup-node", "v3"),
            Some(&"789xyz012".to_string())
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
    fn test_save_without_path_fails() {
        let lock = FileLock::default();
        let result = lock.save();
        assert!(result.is_err());
    }

    #[test]
    fn test_set_and_get() {
        let mut lock = FileLock::default();
        lock.set("actions/checkout", "v4", "abc123".to_string());

        assert_eq!(
            lock.get("actions/checkout", "v4"),
            Some(&"abc123".to_string())
        );
        assert_eq!(lock.get("actions/checkout", "v3"), None);
    }

    #[test]
    fn test_has() {
        let mut lock = FileLock::default();
        lock.set("actions/checkout", "v4", "abc123".to_string());

        assert!(lock.has("actions/checkout", "v4"));
        assert!(!lock.has("actions/checkout", "v3"));
        assert!(!lock.has("actions/setup-node", "v4"));
    }

    #[test]
    fn test_remove_unused() {
        let mut lock = FileLock::default();
        lock.set("actions/checkout", "v4", "abc123".to_string());
        lock.set("actions/setup-node", "v3", "def456".to_string());
        lock.set("actions/old-action", "v1", "xyz789".to_string());

        let mut used = HashMap::new();
        used.insert("actions/checkout".to_string(), "v4".to_string());
        used.insert("actions/setup-node".to_string(), "v3".to_string());

        lock.remove_unused(&used);

        assert!(lock.has("actions/checkout", "v4"));
        assert!(lock.has("actions/setup-node", "v3"));
        assert!(!lock.has("actions/old-action", "v1"));
    }

    #[test]
    fn test_update_existing_sha() {
        let mut lock = FileLock::default();
        lock.set("actions/checkout", "v4", "old_sha".to_string());
        lock.set("actions/checkout", "v4", "new_sha".to_string());

        assert_eq!(
            lock.get("actions/checkout", "v4"),
            Some(&"new_sha".to_string())
        );
    }

    #[test]
    fn test_build_update_map() {
        let mut lock = FileLock::default();
        lock.set("actions/checkout", "v4", "abc123def456".to_string());
        lock.set("actions/setup-node", "v3", "789xyz012".to_string());

        let mut manifest_actions = HashMap::new();
        manifest_actions.insert("actions/checkout".to_string(), "v4".to_string());
        manifest_actions.insert("actions/setup-node".to_string(), "v3".to_string());

        let update_map = lock.build_update_map(&manifest_actions);

        assert_eq!(
            update_map.get("actions/checkout"),
            Some(&"abc123def456 # v4".to_string())
        );
        assert_eq!(
            update_map.get("actions/setup-node"),
            Some(&"789xyz012 # v3".to_string())
        );
    }

    #[test]
    fn test_build_update_map_fallback_to_version() {
        let lock = FileLock::default(); // Empty lock file

        let mut manifest_actions = HashMap::new();
        manifest_actions.insert("actions/checkout".to_string(), "v4".to_string());

        let update_map = lock.build_update_map(&manifest_actions);

        // Should fallback to version if SHA not in lock file
        assert_eq!(update_map.get("actions/checkout"), Some(&"v4".to_string()));
    }
}

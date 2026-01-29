use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::LockFilePathNotInitialized;

const LOCK_FILE_NAME: &str = "gx.lock";

/// Lock file structure that maps action@version to resolved commit SHA
#[derive(Debug, Deserialize, Serialize)]
pub struct LockFile {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub actions: HashMap<String, String>,
    #[serde(skip)]
    path: Option<PathBuf>,
    #[serde(skip)]
    changed: bool,
}

impl LockFile {
    pub fn path(&self) -> Result<&Path> {
        self.path
            .as_ref()
            .map(|p| p.as_path())
            .ok_or_else(|| anyhow!(LockFilePathNotInitialized))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read lock file: {}", path.display()))?;

        let mut lock: LockFile = toml::from_str(&content)
            .with_context(|| format!("Failed to parse lock file: {}", path.display()))?;

        lock.path = Some(path.to_path_buf());
        Ok(lock)
    }

    pub fn load_or_default(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            let mut lock = Self::default();
            lock.path = Some(path.to_path_buf());
            Ok(lock)
        }
    }

    pub fn load_from_repo_or_default(repo_root: &Path) -> Result<Self> {
        let lock_path = repo_root.join(".github").join(LOCK_FILE_NAME);
        Self::load_or_default(&lock_path)
    }

    pub fn save(&self) -> Result<()> {
        let path = self.path()?;
        let content =
            toml::to_string_pretty(self).context("Failed to serialize lock file to TOML")?;

        fs::write(path, content)
            .with_context(|| format!("Failed to write lock file: {}", path.display()))?;

        println!("Lock file updated: {}", path.display());
        Ok(())
    }

    /// Save the lock file only if there were changes
    pub fn save_if_changed(&self) -> Result<()> {
        if self.changed {
            self.save()
        } else {
            Ok(())
        }
    }

    /// Set or update a locked action version
    pub fn set(&mut self, action: &str, version: &str, commit_sha: String) {
        let key = format!("{}@{}", action, version);
        let existing = self.actions.get(&key);
        if existing != Some(&commit_sha) {
            self.actions.insert(key, commit_sha);
            self.changed = true;
        }
    }

    /// Get the locked commit SHA for an action@version
    pub fn get(&self, action: &str, version: &str) -> Option<&String> {
        let key = format!("{}@{}", action, version);
        self.actions.get(&key)
    }

    /// Remove entries for actions no longer in use
    pub fn remove_unused(&mut self, used_actions: &HashMap<String, String>) {
        let used_keys: std::collections::HashSet<String> = used_actions
            .iter()
            .map(|(action, version)| format!("{}@{}", action, version))
            .collect();

        let original_len = self.actions.len();
        self.actions.retain(|key, _| used_keys.contains(key));
        if self.actions.len() != original_len {
            self.changed = true;
        }
    }

    /// Check if lock file has an entry for the given action@version
    pub fn has(&self, action: &str, version: &str) -> bool {
        let key = format!("{}@{}", action, version);
        self.actions.contains_key(&key)
    }

    /// Build a map of action names to "SHA # version" for workflow updates
    /// Takes versions from the manifest and SHAs from the lock file
    pub fn build_update_map(
        &self,
        manifest_actions: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut update_map = HashMap::new();

        for (action, version) in manifest_actions {
            if let Some(sha) = self.get(action, version) {
                // Format as "SHA # version" for the workflow update
                let update_value = format!("{} # {}", sha, version);
                update_map.insert(action.clone(), update_value);
            } else {
                // Fallback to version if SHA not found in lock file
                update_map.insert(action.clone(), version.clone());
            }
        }

        update_map
    }
}

impl Default for LockFile {
    fn default() -> Self {
        Self {
            actions: HashMap::new(),
            path: None,
            changed: false,
        }
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

        let lock = LockFile::load(file.path()).unwrap();

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

        let lock = LockFile::load(file.path()).unwrap();
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

        let lock = LockFile::load_or_default(file.path()).unwrap();
        assert_eq!(
            lock.get("actions/checkout", "v4"),
            Some(&"abc123".to_string())
        );
    }

    #[test]
    fn test_load_or_default_missing() {
        let lock = LockFile::load_or_default(Path::new("/nonexistent/path/gx.lock")).unwrap();
        assert!(lock.actions.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let mut lock = LockFile::default();
        lock.set("actions/checkout", "v4", "abc123def456".to_string());
        lock.set("actions/setup-node", "v3", "789xyz012".to_string());

        let file = NamedTempFile::new().unwrap();
        lock.path = Some(file.path().to_path_buf());
        lock.save().unwrap();

        let loaded = LockFile::load(file.path()).unwrap();
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
        let lock = LockFile::default();
        let result = lock.path();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("LockFile path not initialized"));
    }

    #[test]
    fn test_save_without_path_fails() {
        let lock = LockFile::default();
        let result = lock.save();
        assert!(result.is_err());
    }

    #[test]
    fn test_set_and_get() {
        let mut lock = LockFile::default();
        lock.set("actions/checkout", "v4", "abc123".to_string());

        assert_eq!(
            lock.get("actions/checkout", "v4"),
            Some(&"abc123".to_string())
        );
        assert_eq!(lock.get("actions/checkout", "v3"), None);
    }

    #[test]
    fn test_has() {
        let mut lock = LockFile::default();
        lock.set("actions/checkout", "v4", "abc123".to_string());

        assert!(lock.has("actions/checkout", "v4"));
        assert!(!lock.has("actions/checkout", "v3"));
        assert!(!lock.has("actions/setup-node", "v4"));
    }

    #[test]
    fn test_remove_unused() {
        let mut lock = LockFile::default();
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
        let mut lock = LockFile::default();
        lock.set("actions/checkout", "v4", "old_sha".to_string());
        lock.set("actions/checkout", "v4", "new_sha".to_string());

        assert_eq!(
            lock.get("actions/checkout", "v4"),
            Some(&"new_sha".to_string())
        );
    }

    #[test]
    fn test_build_update_map() {
        let mut lock = LockFile::default();
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
        let lock = LockFile::default(); // Empty lock file

        let mut manifest_actions = HashMap::new();
        manifest_actions.insert("actions/checkout".to_string(), "v4".to_string());

        let update_map = lock.build_update_map(&manifest_actions);

        // Should fallback to version if SHA not in lock file
        assert_eq!(update_map.get("actions/checkout"), Some(&"v4".to_string()));
    }
}

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const LOCK_FILE_NAME: &'static str = "gx.lock";

/// Lock file structure that maps action@version to resolved commit SHA
#[derive(Debug, Deserialize, Serialize)]
pub struct LockFile {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub actions: HashMap<String, String>,
}

impl LockFile {
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read lock file: {}", path.display()))?;

        let lock: LockFile = toml::from_str(&content)
            .with_context(|| format!("Failed to parse lock file: {}", path.display()))?;

        Ok(lock)
    }

    pub fn load_or_default(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            Ok(Self::default())
        }
    }

    pub fn load_from_repo_or_default(repo_root: &Path) -> Result<Self> {
        let lock_path = repo_root.join(".github").join(LOCK_FILE_NAME);
        Self::load_or_default(&lock_path)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content =
            toml::to_string_pretty(self).context("Failed to serialize lock file to TOML")?;

        fs::write(path, content)
            .with_context(|| format!("Failed to write lock file: {}", path.display()))?;

        Ok(())
    }

    pub fn save_to_repo(&self, repo_root: &Path) -> Result<()> {
        let lock_path = repo_root.join(".github").join(LOCK_FILE_NAME);
        println!("Lock file updated: {}", lock_path.display());
        self.save(&lock_path)
    }

    /// Set or update a locked action version
    pub fn set(&mut self, action: &str, version: &str, commit_sha: String) {
        let key = format!("{}@{}", action, version);
        self.actions.insert(key, commit_sha);
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

        self.actions.retain(|key, _| used_keys.contains(key));
    }

    /// Check if lock file has an entry for the given action@version
    pub fn has(&self, action: &str, version: &str) -> bool {
        let key = format!("{}@{}", action, version);
        self.actions.contains_key(&key)
    }
}

impl Default for LockFile {
    fn default() -> Self {
        Self {
            actions: HashMap::new(),
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
        lock.save(file.path()).unwrap();

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
}

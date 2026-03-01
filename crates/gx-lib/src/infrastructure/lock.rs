use log::info;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::domain::{CommitSha, Lock, LockEntry, LockKey, RefType};

pub const LOCK_FILE_NAME: &str = "gx.lock";
pub const LOCK_FILE_VERSION: &str = "1.1";

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

/// Action entry data in the lock file
#[derive(Debug, Clone, Deserialize)]
struct ActionEntryData {
    sha: String,
    repository: String,
    ref_type: String,
    date: String,
}

/// Internal structure for TOML deserialization
#[derive(Debug, Deserialize)]
struct LockData {
    #[serde(default)]
    version: String,
    #[serde(default)]
    actions: HashMap<String, ActionEntryData>,
}

impl Default for LockData {
    fn default() -> Self {
        Self {
            version: LOCK_FILE_VERSION.to_string(),
            actions: HashMap::new(),
        }
    }
}

/// Legacy v1.0 format: actions are plain strings (SHA only)
#[derive(Debug, Deserialize)]
struct LockDataV1 {
    #[serde(default)]
    #[allow(dead_code)]
    version: String,
    #[serde(default)]
    actions: HashMap<String, String>,
}

/// Migrate a v1.0 lock data to v2.0 format using default metadata.
fn migrate_v1(data: LockDataV1) -> LockData {
    let actions = data
        .actions
        .into_iter()
        .map(|(key, sha)| {
            let repository = LockKey::parse(&key)
                .map(|k| k.id.base_repo())
                .unwrap_or_default();
            let entry = ActionEntryData {
                sha,
                repository,
                ref_type: "tag".to_string(),
                date: String::new(),
            };
            (key, entry)
        })
        .collect();
    LockData {
        version: LOCK_FILE_VERSION.to_string(),
        actions,
    }
}

fn lock_from_data(data: LockData) -> Lock {
    let actions = data
        .actions
        .into_iter()
        .filter_map(|(k, entry_data)| {
            LockKey::parse(&k).map(|key| {
                let entry = LockEntry::new(
                    CommitSha::from(entry_data.sha),
                    entry_data.repository,
                    RefType::from(entry_data.ref_type),
                    entry_data.date,
                );
                (key, entry)
            })
        })
        .collect();
    Lock::new(actions)
}

/// Serialize a Lock to TOML format with inline tables.
/// Entries are sorted by key for deterministic output.
fn serialize_lock(lock: &Lock) -> String {
    use std::fmt::Write as _;

    let mut entries: Vec<_> = lock.entries().collect();
    entries.sort_by_key(|(k, _)| k.to_string());

    let mut out = format!("version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n");
    for (key, entry) in entries {
        let _ = writeln!(
            out,
            "\"{key}\" = {{ sha = \"{}\", repository = \"{}\", ref_type = \"{}\", date = \"{}\" }}",
            entry.sha, entry.repository, entry.ref_type, entry.date
        );
    }
    out
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
    /// Returns [`LockFileError::Write`] if the file cannot be written.
    pub fn save(&self, lock: &Lock) -> Result<(), LockFileError> {
        let content = serialize_lock(lock);
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

    // Try parsing as v2.0 format first; if that fails, try v1.0 (plain string values)
    let (data, needs_rewrite) = if let Ok(d) = toml::from_str::<LockData>(&content) {
        let rewrite = d.version != LOCK_FILE_VERSION;
        (d, rewrite)
    } else {
        // Try v1.0 format
        let v1: LockDataV1 = toml::from_str(&content).map_err(|source| LockFileError::Parse {
            path: path.to_path_buf(),
            source: Box::new(source),
        })?;
        info!("Migrating lock file from v1.0 to v{LOCK_FILE_VERSION}");
        (migrate_v1(v1), true)
    };

    let lock = lock_from_data(data);

    if needs_rewrite {
        let store = FileLock::new(path);
        store.save(&lock)?;
    }

    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ActionId, CommitSha, LockKey, RefType, ResolvedAction, Version};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_resolved(action: &str, version: &str, sha: &str) -> ResolvedAction {
        ResolvedAction::new(
            ActionId::from(action),
            Version::from(version),
            CommitSha::from(sha),
            ActionId::from(action).base_repo(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
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
        let entry = loaded.get(&make_key("actions/checkout", "v4"));
        assert!(entry.is_some());
        assert_eq!(
            entry.unwrap().sha,
            CommitSha::from("abc123def456789012345678901234567890abcd")
        );
    }

    #[test]
    fn test_file_lock_load_existing_toml() {
        let content = format!(
            r#"version = "{LOCK_FILE_VERSION}"

[actions]
"actions/checkout@v4" = {{ sha = "abc123def456789012345678901234567890abcd", repository = "actions/checkout", ref_type = "tag", date = "2026-01-01T00:00:00Z" }}
"#
        );
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let lock = parse_lock(file.path()).unwrap();
        let entry = lock.get(&make_key("actions/checkout", "v4"));
        assert!(entry.is_some());
        assert_eq!(
            entry.unwrap().sha,
            CommitSha::from("abc123def456789012345678901234567890abcd")
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
    fn test_file_lock_save_sorts_actions_alphabetically() {
        let file = NamedTempFile::new().unwrap();
        let store = FileLock::new(file.path());

        let mut lock = Lock::default();
        // Insert in non-alphabetical order
        lock.set(&make_resolved(
            "docker/build-push-action",
            "v5",
            "def456789012345678901234567890abcdef123456",
        ));
        lock.set(&make_resolved(
            "actions/checkout",
            "v4",
            "abc123def456789012345678901234567890abcdef",
        ));
        lock.set(&make_resolved(
            "actions-rust-lang/rustfmt",
            "v1",
            "111222333444555666777888999000aaabbbcccddd",
        ));

        store.save(&lock).unwrap();

        let content = fs::read_to_string(file.path()).unwrap();
        let action_lines: Vec<&str> = content
            .lines()
            .filter(|l| l.trim().starts_with('"') && l.contains(" = "))
            .collect();

        let mut sorted = action_lines.clone();
        sorted.sort_unstable();
        assert_eq!(action_lines, sorted);
        assert!(action_lines[0].contains("actions-rust-lang/rustfmt"));
        assert!(action_lines[1].contains("actions/checkout"));
        assert!(action_lines[2].contains("docker/build-push-action"));
    }

    #[test]
    fn test_parse_lock_missing_returns_empty() {
        let lock = parse_lock(Path::new("/nonexistent/gx.lock")).unwrap();
        assert!(!lock.has(&make_key("actions/checkout", "v4")));
    }

    #[test]
    fn test_parse_lock_reads_file() {
        let content = format!(
            "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@v4\" = {{ sha = \"abc123\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"\" }}\n"
        );
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let lock = parse_lock(file.path()).unwrap();
        assert!(lock.has(&make_key("actions/checkout", "v4")));
    }

    #[test]
    fn test_serialize_lock_uses_inline_tables() {
        let mut lock = Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "v4",
            "abc123def456789012345678901234567890abcd",
        ));
        lock.set(&make_resolved(
            "actions/upload-artifact",
            "v6",
            "def456789012345678901234567890abcdef4567",
        ));

        let output = serialize_lock(&lock);

        // Verify structure: version, blank line, [actions], then inline table entries
        assert!(output.contains("version = \"1.1\""));
        assert!(output.contains("[actions]"));
        // Verify inline tables (one line per entry with all fields)
        assert!(output.contains(
            "\"actions/checkout@v4\" = { sha = \"abc123def456789012345678901234567890abcd\""
        ));
        assert!(output.contains(
            "\"actions/upload-artifact@v6\" = { sha = \"def456789012345678901234567890abcdef4567\""
        ));
        // Verify entries are NOT in expanded table format (no multiple [actions."key"] headers)
        assert!(!output.contains("[actions.\"actions/checkout@v4\"]"));
        // Verify entries are sorted
        let checkout_pos = output.find("actions/checkout").unwrap();
        let upload_pos = output.find("actions/upload-artifact").unwrap();
        assert!(checkout_pos < upload_pos);
    }
}

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use toml_edit::DocumentMut;

use crate::domain::{CommitSha, Lock, LockDiff, LockEntry, LockKey, RefType};

pub const LOCK_FILE_NAME: &str = "gx.lock";
pub const LOCK_FILE_VERSION: &str = "1.3";

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

    #[error("invalid lock file: {0}")]
    Validation(String),
}

/// Action entry data in the lock file
#[derive(Debug, Clone, Deserialize)]
struct ActionEntryData {
    sha: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    specifier: Option<String>,
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
                version: None,
                specifier: None,
                repository,
                ref_type: String::new(),
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
                let entry = LockEntry::with_version_and_specifier(
                    CommitSha::from(entry_data.sha),
                    entry_data.version,
                    entry_data.specifier,
                    entry_data.repository,
                    RefType::parse(&entry_data.ref_type),
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
/// Always outputs all 6 fields: sha, version, specifier, repository, `ref_type`, date.
fn serialize_lock(lock: &Lock) -> String {
    use std::fmt::Write as _;

    let mut entries: Vec<_> = lock.entries().collect();
    entries.sort_by_key(|(k, _)| k.to_string());

    let mut out = format!("version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n");
    for (key, entry) in entries {
        // Use lock key version as fallback for version field
        let version = entry.version.as_deref().unwrap_or(key.version.as_str());
        // Use empty string as fallback for specifier
        let specifier = entry.specifier.as_deref().unwrap_or("");

        let table = format!(
            "sha = \"{}\", version = \"{}\", specifier = \"{}\", repository = \"{}\", ref_type = \"{}\", date = \"{}\"",
            entry.sha,
            version,
            specifier,
            entry.repository,
            entry.ref_type.as_ref().map_or("unknown", |r| match r {
                RefType::Release => "release",
                RefType::Tag => "tag",
                RefType::Branch => "branch",
                RefType::Commit => "commit",
            }),
            entry.date
        );

        let _ = writeln!(out, "\"{key}\" = {{ {table} }}");
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
        Ok(())
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
        (migrate_v1(v1), true)
    };

    let lock = lock_from_data(data);

    if needs_rewrite {
        let store = FileLock::new(path);
        store.save(&lock)?;
    }

    Ok(lock)
}

/// Build a `toml_edit::InlineTable` from a lock key and entry for insertion.
fn build_lock_inline_table(key: &LockKey, entry: &LockEntry) -> toml_edit::InlineTable {
    let version = entry.version.as_deref().unwrap_or(key.version.as_str());
    let specifier = entry.specifier.as_deref().unwrap_or("");

    let mut inline = toml_edit::InlineTable::new();
    inline.insert("sha", entry.sha.as_str().into());
    inline.insert("version", version.into());
    inline.insert("specifier", specifier.into());
    inline.insert("repository", entry.repository.as_str().into());
    inline.insert(
        "ref_type",
        entry
            .ref_type
            .as_ref()
            .map_or("unknown".to_string(), std::string::ToString::to_string)
            .as_str()
            .into(),
    );
    inline.insert("date", entry.date.as_str().into());
    inline
}

/// Create a new lock file from a `LockDiff`.
///
/// This builds a fresh lock file from the `added` entries.
/// Used for the `init` command when no lock file exists yet.
///
/// # Errors
///
/// Returns [`LockFileError::Write`] if the file cannot be written.
pub fn create_lock(path: &Path, diff: &LockDiff) -> Result<(), LockFileError> {
    use std::fmt::Write as _;

    // Build sorted entries
    let mut entries: Vec<_> = diff.added.iter().collect();
    entries.sort_by_key(|(k, _)| k.to_string());

    let mut out = format!("version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n");
    for (key, entry) in entries {
        let version = entry.version.as_deref().unwrap_or(key.version.as_str());
        let specifier = entry.specifier.as_deref().unwrap_or("");

        let table = format!(
            "sha = \"{}\", version = \"{}\", specifier = \"{}\", repository = \"{}\", ref_type = \"{}\", date = \"{}\"",
            entry.sha,
            version,
            specifier,
            entry.repository,
            entry.ref_type.as_ref().map_or("unknown", |r| match r {
                RefType::Release => "release",
                RefType::Tag => "tag",
                RefType::Branch => "branch",
                RefType::Commit => "commit",
            }),
            entry.date
        );

        let _ = writeln!(out, "\"{key}\" = {{ {table} }}");
    }

    fs::write(path, out).map_err(|source| LockFileError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Apply a `LockDiff` to an existing lock file using `toml_edit` for surgical patching.
///
/// The file must already exist. For creating a new lock from scratch, use `create_lock`.
///
/// # Errors
///
/// Returns [`LockFileError::Read`] if the file cannot be read.
/// Returns [`LockFileError::Write`] if the file cannot be written.
/// Returns [`LockFileError::Parse`] if the file cannot be parsed as TOML.
pub fn apply_lock_diff(path: &Path, diff: &LockDiff) -> Result<(), LockFileError> {
    if diff.is_empty() {
        return Ok(());
    }

    let content = fs::read_to_string(path).map_err(|source| LockFileError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let mut doc: DocumentMut = content
        .parse()
        .map_err(|e| LockFileError::Validation(format!("toml_edit parse error: {e}")))?;

    // Ensure [actions] table exists
    if doc.get("actions").is_none() {
        doc["actions"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let Some(actions) = doc["actions"].as_table_mut() else {
        return Err(LockFileError::Validation(
            "[actions] is not a table".to_string(),
        ));
    };

    // Remove entries
    for key in &diff.removed {
        actions.remove(&key.to_string());
    }

    // Add entries
    for (key, entry) in &diff.added {
        actions.insert(
            &key.to_string(),
            toml_edit::value(build_lock_inline_table(key, entry)),
        );
    }

    // Update existing entries (patch specific fields)
    for (key, patch) in &diff.updated {
        let key_str = key.to_string();
        if let Some(item) = actions.get_mut(&key_str)
            && let Some(inline) = item.as_inline_table_mut()
        {
            if let Some(version) = &patch.version {
                match version {
                    Some(v) => inline.insert("version", v.as_str().into()),
                    None => inline.remove("version"),
                };
            }
            if let Some(specifier) = &patch.specifier {
                match specifier {
                    Some(s) => inline.insert("specifier", s.as_str().into()),
                    None => inline.insert("specifier", "".into()),
                };
            }
        }
    }

    actions.sort_values();

    fs::write(path, doc.to_string()).map_err(|source| LockFileError::Write {
        path: path.to_path_buf(),
        source,
    })?;

    Ok(())
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
            Some(RefType::Tag),
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
        assert!(output.contains("version = \"1.3\""));
        assert!(output.contains("[actions]"));
        // Verify inline tables (one line per entry with all 6 fields)
        assert!(output.contains(
            "\"actions/checkout@v4\" = { sha = \"abc123def456789012345678901234567890abcd\", version = \"v4\", specifier ="
        ));
        assert!(output.contains(
            "\"actions/upload-artifact@v6\" = { sha = \"def456789012345678901234567890abcdef4567\", version = \"v6\", specifier ="
        ));
        // Verify entries are NOT in expanded table format (no multiple [actions."key"] headers)
        assert!(!output.contains("[actions.\"actions/checkout@v4\"]"));
        // Verify entries are sorted
        let checkout_pos = output.find("actions/checkout").unwrap();
        let upload_pos = output.find("actions/upload-artifact").unwrap();
        assert!(checkout_pos < upload_pos);
    }

    #[test]
    fn test_roundtrip_with_version_and_specifier() {
        use std::io::Write as StdWrite;

        let file = NamedTempFile::new().unwrap();

        // Create a lock entry with version and specifier
        let content = r#"version = "1.3"

[actions]
"actions/checkout@v6" = { sha = "de0fac2e4500dabe0009e67214ff5f5447ce83dd", version = "v6.2.3", specifier = "^6", repository = "actions/checkout", ref_type = "release", date = "2026-01-09T19:42:23Z" }
"#;
        let mut f = std::fs::File::create(file.path()).unwrap();
        f.write_all(content.as_bytes()).unwrap();

        // Load and verify the new fields are preserved
        let lock = parse_lock(file.path()).unwrap();
        let entry = lock.get(&make_key("actions/checkout", "v6"));
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.version, Some("v6.2.3".to_string()));
        assert_eq!(entry.specifier, Some("^6".to_string()));
    }

    // ========== Step 12: apply_lock_diff tests ==========

    use crate::domain::LockDiff;

    #[test]
    fn test_apply_lock_empty_diff_does_not_modify_file() {
        let content = format!(
            "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@v4\" = {{ sha = \"abc123def456789012345678901234567890abcd\", version = \"v4\", specifier = \"\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"2026-01-01T00:00:00Z\" }}\n"
        );
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = LockDiff::default();
        apply_lock_diff(file.path(), &diff).unwrap();

        let after = fs::read_to_string(file.path()).unwrap();
        assert_eq!(content, after, "Empty diff must not modify file");
    }

    #[test]
    fn test_apply_lock_add_one_entry() {
        let content = format!(
            "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@v4\" = {{ sha = \"abc123def456789012345678901234567890abcd\", version = \"v4\", specifier = \"^4\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"2026-01-01T00:00:00Z\" }}\n"
        );
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = LockDiff {
            added: vec![(
                make_key("actions/setup-node", "v3"),
                LockEntry::with_version_and_specifier(
                    CommitSha::from("def456789012345678901234567890abcdef1234"),
                    Some("v3.1.0".to_string()),
                    Some("^3".to_string()),
                    "actions/setup-node".to_string(),
                    Some(RefType::Tag),
                    "2026-01-01T00:00:00Z".to_string(),
                ),
            )],
            ..Default::default()
        };
        apply_lock_diff(file.path(), &diff).unwrap();

        // Round-trip
        let loaded = parse_lock(file.path()).unwrap();
        assert!(
            loaded.has(&make_key("actions/checkout", "v4")),
            "Existing entry preserved"
        );
        let entry = loaded
            .get(&make_key("actions/setup-node", "v3"))
            .expect("New entry exists");
        assert_eq!(
            entry.sha,
            CommitSha::from("def456789012345678901234567890abcdef1234")
        );
        assert_eq!(entry.version, Some("v3.1.0".to_string()));
        assert_eq!(entry.specifier, Some("^3".to_string()));
        assert_eq!(entry.repository, "actions/setup-node");
    }

    #[test]
    fn test_apply_lock_remove_one_entry() {
        let content = format!(
            "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@v4\" = {{ sha = \"abc123\", version = \"v4\", specifier = \"\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"\" }}\n\"actions/setup-node@v3\" = {{ sha = \"def456\", version = \"v3\", specifier = \"\", repository = \"actions/setup-node\", ref_type = \"tag\", date = \"\" }}\n"
        );
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = LockDiff {
            removed: vec![make_key("actions/checkout", "v4")],
            ..Default::default()
        };
        apply_lock_diff(file.path(), &diff).unwrap();

        let loaded = parse_lock(file.path()).unwrap();
        assert!(!loaded.has(&make_key("actions/checkout", "v4")));
        assert!(loaded.has(&make_key("actions/setup-node", "v3")));
    }

    #[test]
    fn test_apply_lock_update_version_field() {
        let content = format!(
            "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@v4\" = {{ sha = \"abc123\", version = \"v4\", specifier = \"^4\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"2026-01-01T00:00:00Z\" }}\n"
        );
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = LockDiff {
            updated: vec![(
                make_key("actions/checkout", "v4"),
                crate::domain::LockEntryPatch {
                    version: Some(Some("v4.1.0".to_string())),
                    specifier: None,
                },
            )],
            ..Default::default()
        };
        apply_lock_diff(file.path(), &diff).unwrap();

        let loaded = parse_lock(file.path()).unwrap();
        let entry = loaded.get(&make_key("actions/checkout", "v4")).unwrap();
        assert_eq!(entry.version, Some("v4.1.0".to_string()));
        assert_eq!(
            entry.specifier,
            Some("^4".to_string()),
            "Specifier must be unchanged"
        );
    }

    #[test]
    fn test_apply_lock_update_specifier_field() {
        let content = format!(
            "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@v4\" = {{ sha = \"abc123\", version = \"v4\", specifier = \"^4\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"2026-01-01T00:00:00Z\" }}\n"
        );
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = LockDiff {
            updated: vec![(
                make_key("actions/checkout", "v4"),
                crate::domain::LockEntryPatch {
                    version: None,
                    specifier: Some(Some("^4.1".to_string())),
                },
            )],
            ..Default::default()
        };
        apply_lock_diff(file.path(), &diff).unwrap();

        let loaded = parse_lock(file.path()).unwrap();
        let entry = loaded.get(&make_key("actions/checkout", "v4")).unwrap();
        assert_eq!(
            entry.version,
            Some("v4".to_string()),
            "Version must be unchanged"
        );
        assert_eq!(entry.specifier, Some("^4.1".to_string()));
    }

    #[test]
    fn test_apply_lock_roundtrip() {
        let content = format!(
            "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@v4\" = {{ sha = \"abc123def456789012345678901234567890abcd\", version = \"v4\", specifier = \"^4\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"2026-01-01T00:00:00Z\" }}\n\"actions/old-action@v1\" = {{ sha = \"aaabbbcccdddeeefffaaabbbcccdddeeefffaaab\", version = \"v1\", specifier = \"\", repository = \"actions/old-action\", ref_type = \"tag\", date = \"\" }}\n"
        );
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = LockDiff {
            added: vec![(
                make_key("actions/setup-node", "v3"),
                LockEntry::with_version_and_specifier(
                    CommitSha::from("def456789012345678901234567890abcdef1234"),
                    Some("v3.1.0".to_string()),
                    Some("^3".to_string()),
                    "actions/setup-node".to_string(),
                    Some(RefType::Tag),
                    "2026-01-01T00:00:00Z".to_string(),
                ),
            )],
            removed: vec![make_key("actions/old-action", "v1")],
            updated: vec![],
        };
        apply_lock_diff(file.path(), &diff).unwrap();

        let loaded = parse_lock(file.path()).unwrap();
        assert!(loaded.has(&make_key("actions/checkout", "v4")));
        assert!(loaded.has(&make_key("actions/setup-node", "v3")));
        assert!(!loaded.has(&make_key("actions/old-action", "v1")));
    }

    // ========== Step 13: create_lock tests ==========

    #[test]
    fn test_create_lock_from_diff_with_3_entries() {
        let file = NamedTempFile::new().unwrap();

        let diff = LockDiff {
            added: vec![
                (
                    make_key("actions/checkout", "v4"),
                    LockEntry::with_version_and_specifier(
                        CommitSha::from("abc123def456789012345678901234567890abcd"),
                        Some("v4.1.0".to_string()),
                        Some("^4".to_string()),
                        "actions/checkout".to_string(),
                        Some(RefType::Tag),
                        "2026-01-01T00:00:00Z".to_string(),
                    ),
                ),
                (
                    make_key("actions/setup-node", "v3"),
                    LockEntry::with_version_and_specifier(
                        CommitSha::from("def456789012345678901234567890abcdef1234"),
                        Some("v3.2.0".to_string()),
                        Some("^3".to_string()),
                        "actions/setup-node".to_string(),
                        Some(RefType::Tag),
                        "2026-01-01T00:00:00Z".to_string(),
                    ),
                ),
                (
                    make_key("actions/cache", "v3"),
                    LockEntry::with_version_and_specifier(
                        CommitSha::from("111222333444555666777888999000aaabbbcccddd"),
                        Some("v3.0.0".to_string()),
                        Some("^3".to_string()),
                        "actions/cache".to_string(),
                        Some(RefType::Tag),
                        "2026-01-01T00:00:00Z".to_string(),
                    ),
                ),
            ],
            ..Default::default()
        };
        create_lock(file.path(), &diff).unwrap();

        let content = fs::read_to_string(file.path()).unwrap();
        assert!(content.contains(&format!("version = \"{LOCK_FILE_VERSION}\"")));
        assert!(content.contains("[actions]"));

        // Round-trip
        let loaded = parse_lock(file.path()).unwrap();
        assert!(loaded.has(&make_key("actions/checkout", "v4")));
        assert!(loaded.has(&make_key("actions/setup-node", "v3")));
        assert!(loaded.has(&make_key("actions/cache", "v3")));
        let entry = loaded.get(&make_key("actions/checkout", "v4")).unwrap();
        assert_eq!(
            entry.sha,
            CommitSha::from("abc123def456789012345678901234567890abcd")
        );
        assert_eq!(entry.version, Some("v4.1.0".to_string()));
        assert_eq!(entry.specifier, Some("^4".to_string()));
    }

    #[test]
    fn test_create_lock_roundtrip_matches_domain_state() {
        let file = NamedTempFile::new().unwrap();

        let key = make_key("actions/checkout", "v4");
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4.2.0".to_string()),
            Some("^4".to_string()),
            "actions/checkout".to_string(),
            Some(RefType::Release),
            "2026-01-15T10:30:00Z".to_string(),
        );

        let diff = LockDiff {
            added: vec![(key.clone(), entry.clone())],
            ..Default::default()
        };
        create_lock(file.path(), &diff).unwrap();

        let loaded = parse_lock(file.path()).unwrap();
        let loaded_entry = loaded.get(&key).expect("Entry must exist");
        assert_eq!(loaded_entry.sha, entry.sha);
        assert_eq!(loaded_entry.version, entry.version);
        assert_eq!(loaded_entry.specifier, entry.specifier);
        assert_eq!(loaded_entry.repository, entry.repository);
        assert_eq!(loaded_entry.ref_type, entry.ref_type);
        assert_eq!(loaded_entry.date, entry.date);
    }
}

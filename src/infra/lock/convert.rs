use super::LOCK_FILE_VERSION;
use crate::domain::{CommitSha, Lock, LockEntry, LockKey, RefType};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Write as _;

/// Action entry data in the lock file (v1.4)
#[derive(Debug, Clone, Deserialize)]
pub(super) struct ActionEntryData {
    pub(super) sha: String,
    #[serde(default)]
    pub(super) version: Option<String>,
    #[serde(default)]
    pub(super) comment: String,
    pub(super) repository: String,
    pub(super) ref_type: String,
    pub(super) date: String,
}

/// Internal structure for TOML deserialization
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct LockData {
    #[serde(default)]
    pub(super) version: String,
    #[serde(default)]
    pub(super) actions: HashMap<String, ActionEntryData>,
}

impl Default for LockData {
    fn default() -> Self {
        Self {
            version: LOCK_FILE_VERSION.to_string(),
            actions: HashMap::new(),
        }
    }
}

pub(super) fn lock_from_data(data: LockData) -> Lock {
    let actions = data
        .actions
        .into_iter()
        .filter_map(|(k, entry_data)| {
            LockKey::parse(&k).map(|key| {
                let entry = LockEntry::with_version_and_comment(
                    CommitSha::from(entry_data.sha),
                    entry_data.version,
                    entry_data.comment,
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
/// Always outputs 6 fields: sha, version, comment, repository, `ref_type`, date.
pub(super) fn serialize_lock(lock: &Lock) -> String {
    let mut entries: Vec<_> = lock.entries().collect();
    entries.sort_by_key(|(k, _)| k.to_string());

    let mut out = format!("version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n");
    for (key, entry) in entries {
        // Use lock key version string as fallback for version field
        let version = entry.version.as_deref().unwrap_or(key.version.as_str());

        let table = format!(
            "sha = \"{}\", version = \"{}\", comment = \"{}\", repository = \"{}\", ref_type = \"{}\", date = \"{}\"",
            entry.sha,
            version,
            entry.comment,
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

/// Build a `toml_edit::InlineTable` from a lock key and entry for insertion.
pub(super) fn build_lock_inline_table(key: &LockKey, entry: &LockEntry) -> toml_edit::InlineTable {
    let version = entry.version.as_deref().unwrap_or(key.version.as_str());

    let mut inline = toml_edit::InlineTable::new();
    inline.insert("sha", entry.sha.as_str().into());
    inline.insert("version", version.into());
    inline.insert("comment", entry.comment.as_str().into());
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

#[cfg(test)]
mod tests {
    use super::serialize_lock;
    use crate::domain::{ActionId, CommitSha, LockKey, RefType, ResolvedAction, Specifier};
    use crate::infra::lock::{FileLock, LOCK_FILE_VERSION, parse_lock};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_resolved(action: &str, specifier: &str, sha: &str) -> ResolvedAction {
        ResolvedAction::new(
            ActionId::from(action),
            Specifier::parse(specifier),
            CommitSha::from(sha),
            ActionId::from(action).base_repo(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        )
    }

    fn make_key(action: &str, specifier: &str) -> LockKey {
        LockKey::new(ActionId::from(action), Specifier::parse(specifier))
    }

    #[test]
    fn test_file_lock_save_and_load_roundtrip() {
        let file = NamedTempFile::new().unwrap();
        let store = FileLock::new(file.path());

        let mut lock = crate::domain::Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcd",
        ));

        store.save(&lock).unwrap();

        let loaded = parse_lock(file.path()).unwrap();
        let entry = loaded.value.get(&make_key("actions/checkout", "^4"));
        assert!(entry.is_some());
        assert_eq!(
            entry.unwrap().sha,
            CommitSha::from("abc123def456789012345678901234567890abcd")
        );
        assert!(!loaded.migrated);
    }

    #[test]
    fn test_file_lock_load_existing_toml() {
        let content = format!(
            r#"version = "{LOCK_FILE_VERSION}"

[actions]
"actions/checkout@^4" = {{ sha = "abc123def456789012345678901234567890abcd", version = "v4.0.0", comment = "v4", repository = "actions/checkout", ref_type = "tag", date = "2026-01-01T00:00:00Z" }}
"#
        );
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let parsed = parse_lock(file.path()).unwrap();
        let entry = parsed.value.get(&make_key("actions/checkout", "^4"));
        assert!(entry.is_some());
        assert_eq!(
            entry.unwrap().sha,
            CommitSha::from("abc123def456789012345678901234567890abcd")
        );
        assert_eq!(entry.unwrap().comment, "v4");
        assert!(!parsed.migrated);
    }

    #[test]
    fn test_file_lock_save_sorts_actions_alphabetically() {
        let file = NamedTempFile::new().unwrap();
        let store = FileLock::new(file.path());

        let mut lock = crate::domain::Lock::default();
        // Insert in non-alphabetical order
        lock.set(&make_resolved(
            "docker/build-push-action",
            "^5",
            "def456789012345678901234567890abcdef123456",
        ));
        lock.set(&make_resolved(
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcdef",
        ));
        lock.set(&make_resolved(
            "actions-rust-lang/rustfmt",
            "^1",
            "111222333444555666777888999000aaabbbcccddd",
        ));

        store.save(&lock).unwrap();

        let content = std::fs::read_to_string(file.path()).unwrap();
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
    fn test_serialize_lock_uses_inline_tables() {
        let mut lock = crate::domain::Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcd",
        ));
        lock.set(&make_resolved(
            "actions/upload-artifact",
            "^6",
            "def456789012345678901234567890abcdef4567",
        ));

        let output = serialize_lock(&lock);

        // Verify structure: version, blank line, [actions], then inline table entries
        assert!(output.contains("version = \"1.4\""));
        assert!(output.contains("[actions]"));
        // Verify inline tables contain comment field
        assert!(output.contains(
            "\"actions/checkout@^4\" = { sha = \"abc123def456789012345678901234567890abcd\", version ="
        ));
        assert!(output.contains("comment ="));
        // Verify entries are NOT in expanded table format
        assert!(!output.contains("[actions.\"actions/checkout@^4\"]"));
        // Verify entries are sorted
        let checkout_pos = output.find("actions/checkout@").unwrap();
        let upload_pos = output.find("actions/upload-artifact@").unwrap();
        assert!(checkout_pos < upload_pos);
    }

    #[test]
    fn test_roundtrip_with_version_and_comment() {
        let file = NamedTempFile::new().unwrap();

        let content = r#"version = "1.4"

[actions]
"actions/checkout@^6" = { sha = "de0fac2e4500dabe0009e67214ff5f5447ce83dd", version = "v6.2.3", comment = "v6", repository = "actions/checkout", ref_type = "release", date = "2026-01-09T19:42:23Z" }
"#;
        let mut f = std::fs::File::create(file.path()).unwrap();
        f.write_all(content.as_bytes()).unwrap();

        let parsed = parse_lock(file.path()).unwrap();
        let entry = parsed.value.get(&make_key("actions/checkout", "^6"));
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.version, Some("v6.2.3".to_string()));
        assert_eq!(entry.comment, "v6");
        assert!(!parsed.migrated);
    }
}

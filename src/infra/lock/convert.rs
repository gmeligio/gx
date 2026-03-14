use crate::domain::action::identity::CommitSha;
use crate::domain::action::spec::LockKey;
use crate::domain::action::uses_ref::RefType;
use crate::domain::lock::{Lock, entry::Entry as LockEntry};
use serde::Deserialize;
use std::collections::HashMap;
use toml_edit::DocumentMut;

/// Action entry data in the lock file (v1.4 and new format)
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
#[derive(Default)]
pub(super) struct LockData {
    #[serde(default)]
    pub(super) version: String,
    #[serde(default)]
    pub(super) actions: HashMap<String, ActionEntryData>,
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

/// Build a `toml_edit::DocumentMut` from a `Lock` using standard TOML tables.
/// Entries are sorted by key for deterministic output.
/// Each entry has 6 fields: sha, version, comment, repository, `ref_type`, date.
/// No top-level `version` field is written.
pub(super) fn build_lock_document(lock: &Lock) -> DocumentMut {
    let mut doc = DocumentMut::new();

    let mut actions = toml_edit::Table::new();
    actions.set_implicit(true);

    let mut entries: Vec<_> = lock.entries().collect();
    entries.sort_by_key(|(k, _)| k.to_string());

    for (key, entry) in entries {
        let mut table = toml_edit::Table::new();
        populate_lock_table(&mut table, key, entry);
        actions.insert(&key.to_string(), toml_edit::Item::Table(table));
    }

    doc.insert("actions", toml_edit::Item::Table(actions));
    doc
}

/// Populate a standard TOML table with lock entry fields in fixed order.
pub(super) fn populate_lock_table(table: &mut toml_edit::Table, key: &LockKey, entry: &LockEntry) {
    let version = entry.version.as_deref().unwrap_or(key.version.as_str());
    let ref_type_str = entry.ref_type.as_ref().map_or("unknown", |r| match r {
        RefType::Release => "release",
        RefType::Tag => "tag",
        RefType::Branch => "branch",
        RefType::Commit => "commit",
    });

    table.insert("sha", toml_edit::value(entry.sha.as_str()));
    table.insert("version", toml_edit::value(version));
    table.insert("comment", toml_edit::value(entry.comment.as_str()));
    table.insert("repository", toml_edit::value(entry.repository.as_str()));
    table.insert("ref_type", toml_edit::value(ref_type_str));
    table.insert("date", toml_edit::value(entry.date.as_str()));
}

#[cfg(test)]
mod tests {
    use super::build_lock_document;
    use crate::domain::action::identity::{ActionId, CommitSha};
    use crate::domain::action::resolved::Resolved as ResolvedAction;
    use crate::domain::action::spec::LockKey;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::infra::lock::{Store, parse};
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
        let store = Store::new(file.path());

        let mut lock = crate::domain::lock::Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcd",
        ));

        store.save(&lock).unwrap();

        let loaded = parse(file.path()).unwrap();
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
        // New format: standard tables, no version field
        let content = r#"[actions."actions/checkout@^4"]
sha = "abc123def456789012345678901234567890abcd"
version = "v4.0.0"
comment = "v4"
repository = "actions/checkout"
ref_type = "tag"
date = "2026-01-01T00:00:00Z"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let parsed = parse(file.path()).unwrap();
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
        let store = Store::new(file.path());

        let mut lock = crate::domain::lock::Lock::default();
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
        // In standard table format, section headers contain the action keys
        let section_lines: Vec<&str> = content
            .lines()
            .filter(|l| l.starts_with("[actions.\""))
            .collect();

        assert_eq!(section_lines.len(), 3);
        assert!(section_lines[0].contains("actions-rust-lang/rustfmt"));
        assert!(section_lines[1].contains("actions/checkout"));
        assert!(section_lines[2].contains("docker/build-push-action"));
    }

    #[test]
    fn test_build_lock_document_uses_standard_tables() {
        let mut lock = crate::domain::lock::Lock::default();
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

        let output = build_lock_document(&lock).to_string();

        // Verify no version field
        assert!(!output.contains("version = \"1.4\""));
        // Verify standard table format
        assert!(output.contains("[actions.\"actions/checkout@^4\"]"));
        assert!(output.contains("[actions.\"actions/upload-artifact@^6\"]"));
        // Verify fields exist
        assert!(output.contains("sha = \"abc123def456789012345678901234567890abcd\""));
        assert!(output.contains("comment ="));
        // Verify entries are NOT inline
        assert!(!output.contains("{ sha ="));
        // Verify entries are sorted
        let checkout_pos = output.find("actions/checkout@").unwrap();
        let upload_pos = output.find("actions/upload-artifact@").unwrap();
        assert!(checkout_pos < upload_pos);
    }

    #[test]
    fn test_roundtrip_with_version_and_comment() {
        let file = NamedTempFile::new().unwrap();

        // v1.4 format (old) — should be parsed as migration
        let content = r#"version = "1.4"

[actions]
"actions/checkout@^6" = { sha = "de0fac2e4500dabe0009e67214ff5f5447ce83dd", version = "v6.2.3", comment = "v6", repository = "actions/checkout", ref_type = "release", date = "2026-01-09T19:42:23Z" }
"#;
        let mut f = std::fs::File::create(file.path()).unwrap();
        f.write_all(content.as_bytes()).unwrap();

        let parsed = parse(file.path()).unwrap();
        let entry = parsed.value.get(&make_key("actions/checkout", "^6"));
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.version, Some("v6.2.3".to_string()));
        assert_eq!(entry.comment, "v6");
        assert!(parsed.migrated);
    }
}

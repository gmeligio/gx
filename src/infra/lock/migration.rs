use super::LOCK_FILE_VERSION;
use super::convert::{ActionEntryData, LockData};
use crate::domain::action::spec::LockKey;
use crate::domain::action::specifier::Specifier;
use serde::Deserialize;
use std::collections::HashMap;

/// Legacy v1.0 format: actions are plain strings (SHA only)
#[derive(Debug, Deserialize)]
pub(super) struct LockDataV1 {
    #[serde(default)]
    pub(super) actions: HashMap<String, String>,
}

/// v1.3 format: has `specifier` field, keys use `@v6` style
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub(super) struct ActionEntryDataV1_3 {
    pub(super) sha: String,
    #[serde(default)]
    pub(super) version: Option<String>,
    #[serde(default)]
    pub(super) specifier: Option<String>,
    pub(super) repository: String,
    pub(super) ref_type: String,
    pub(super) date: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct LockDataV1_3 {
    #[serde(default)]
    pub(super) version: String,
    #[serde(default)]
    pub(super) actions: HashMap<String, ActionEntryDataV1_3>,
}

/// Migrate v1.0 lock (plain SHA values) to v1.4 format.
pub(super) fn migrate_v1(data: LockDataV1) -> LockData {
    let actions = data
        .actions
        .into_iter()
        .map(|(key, sha)| {
            // Migrate key from @v6 style to @^6 style
            let new_key = migrate_key(&key);
            let comment = derive_comment_from_v1_key(&key);
            let repository = LockKey::parse(&new_key)
                .map(|k| k.id.base_repo())
                .unwrap_or_default();
            let entry = ActionEntryData {
                sha,
                version: None,
                comment,
                repository,
                ref_type: String::new(),
                date: String::new(),
            };
            (new_key, entry)
        })
        .collect();
    LockData {
        version: LOCK_FILE_VERSION.to_string(),
        actions,
    }
}

/// Migrate v1.3 lock (specifier field, @v6 keys) to v1.4 format.
pub(super) fn migrate_v1_3(data: LockDataV1_3) -> LockData {
    let actions = data
        .actions
        .into_iter()
        .map(|(key, entry)| {
            let new_key = migrate_key(&key);
            let comment = derive_comment_from_v1_key(&key);
            let new_entry = ActionEntryData {
                sha: entry.sha,
                version: entry.version,
                comment,
                repository: entry.repository,
                ref_type: entry.ref_type,
                date: entry.date,
            };
            (new_key, new_entry)
        })
        .collect();
    LockData {
        version: LOCK_FILE_VERSION.to_string(),
        actions,
    }
}

/// Convert a v1.x key like "actions/checkout@v6" to v1.4 "actions/checkout@^6".
pub(super) fn migrate_key(key: &str) -> String {
    if let Some((action, version_part)) = key.rsplit_once('@') {
        let specifier = Specifier::from_v1(version_part);
        format!("{action}@{specifier}")
    } else {
        key.to_string()
    }
}

/// Derive the human-readable comment from a v1.x key version part.
/// "v6" → "v6", "v6.1" → "v6.1", "v1.15.2" → "v1.15.2", "main" → ""
pub(super) fn derive_comment_from_v1_key(key: &str) -> String {
    if let Some((_, version_part)) = key.rsplit_once('@') {
        // If it starts with 'v' followed by a digit, use as comment
        if version_part.starts_with('v') {
            return version_part.to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use crate::domain::action::identity::{ActionId, CommitSha};
    use crate::domain::action::spec::LockKey;
    use crate::domain::action::specifier::Specifier;
    use crate::infra::lock::parse;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_key(action: &str, specifier: &str) -> LockKey {
        LockKey::new(ActionId::from(action), Specifier::parse(specifier))
    }

    #[test]
    fn test_file_lock_migrates_v1_0_format() {
        // v1.0 format: plain string SHA values
        let content = r#"
[actions]
"actions/checkout@v4" = "abc123def456789012345678901234567890abcd"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let parsed = parse(file.path()).unwrap();
        assert!(parsed.migrated);
        // Key should have been migrated to ^4
        let entry = parsed.value.get(&make_key("actions/checkout", "^4"));
        assert!(entry.is_some());
        assert_eq!(
            entry.unwrap().sha,
            CommitSha::from("abc123def456789012345678901234567890abcd")
        );
    }

    #[test]
    fn test_file_lock_migrates_v1_3_format() {
        // v1.3 format: specifier field, @v6 style keys
        let content = r#"version = "1.3"

[actions]
"actions/checkout@v6" = { sha = "de0fac2e4500dabe0009e67214ff5f5447ce83dd", version = "v6.2.3", specifier = "^6", repository = "actions/checkout", ref_type = "release", date = "2026-01-09T19:42:23Z" }
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let parsed = parse(file.path()).unwrap();
        assert!(parsed.migrated);
        // Key migrated from @v6 to @^6
        let entry = parsed.value.get(&make_key("actions/checkout", "^6"));
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(
            entry.sha,
            CommitSha::from("de0fac2e4500dabe0009e67214ff5f5447ce83dd")
        );
        assert_eq!(entry.version, Some("v6.2.3".to_string()));
        assert_eq!(entry.comment, "v6"); // derived from old key
    }
}

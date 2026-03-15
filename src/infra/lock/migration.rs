use crate::domain::action::identity::{CommitSha, Version};
use crate::domain::action::resolved::Commit;
use crate::domain::action::spec::Spec;
use crate::domain::action::uses_ref::RefType;
use crate::domain::lock::resolution::Resolution;
use crate::domain::lock::{ActionKey, Lock};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Action entry data in the flat lock format.
#[derive(Debug, Clone, Deserialize)]
pub struct FlatEntryData {
    /// The full commit SHA.
    pub sha: String,
    /// The resolved version string (e.g. "v4.0.0").
    #[serde(default)]
    pub version: Option<String>,
    /// Human-readable version comment (e.g. "v4").
    #[serde(default)]
    pub comment: String,
    /// The source repository (e.g. "actions/checkout").
    pub repository: String,
    /// The ref type that was resolved (tag, branch, commit, or release).
    pub ref_type: String,
    /// ISO 8601 date of the resolved commit or release.
    pub date: String,
}

/// Internal structure for flat TOML deserialization (legacy format).
#[derive(Debug, Deserialize, Default)]
pub struct FlatData {
    /// Schema version string (ignored by serde — present in v1.4 files).
    #[serde(default)]
    #[expect(
        dead_code,
        reason = "version field is consumed by serde but not accessed directly"
    )]
    pub version: String,
    /// Map of `"action@specifier"` composite keys to their entry data.
    #[serde(default)]
    pub actions: HashMap<String, FlatEntryData>,
}

/// Try to parse lock file content as the flat format.
///
/// Returns `Ok(Some(lock))` if the content is flat format (has `[actions` but no `[resolutions`),
/// `Ok(None)` if the content is not flat format, or `Err` if parsing fails.
pub fn try_parse(content: &str, path: &Path) -> Result<Option<Lock>, super::Error> {
    if content.contains("[resolutions") {
        return Ok(None);
    }
    if !content.contains("[actions") {
        return Ok(None);
    }

    let data: FlatData = super::parse_toml(content, path)?;
    Ok(Some(lock_from_flat(data)))
}

/// Convert deserialized flat lock data into a domain `Lock`.
fn lock_from_flat(data: FlatData) -> Lock {
    let mut resolutions = HashMap::new();
    let mut actions = HashMap::new();

    for (k, entry_data) in data.actions {
        let Some(spec) = Spec::parse(&k) else {
            continue;
        };
        let version_str = entry_data
            .version
            .as_deref()
            .unwrap_or(spec.version.as_str());
        let version = Version::from(version_str);
        resolutions.insert(
            spec.clone(),
            Resolution {
                version: version.clone(),
                comment: entry_data.comment,
            },
        );
        let key = ActionKey {
            id: spec.id.clone(),
            version,
        };
        actions.insert(
            key,
            Commit {
                sha: CommitSha::from(entry_data.sha),
                repository: entry_data.repository,
                ref_type: RefType::parse(&entry_data.ref_type),
                date: entry_data.date,
            },
        );
    }

    Lock::new(resolutions, actions)
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::*;
    use crate::domain::action::identity::{ActionId, CommitSha};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;

    fn make_key(action: &str, specifier: &str) -> Spec {
        Spec::new(ActionId::from(action), Specifier::parse(specifier))
    }

    #[test]
    fn flat_format_with_version_field_parses() {
        let content = r#"version = "1.4"

[actions]
"actions/checkout@^6" = { sha = "de0fac2e4500dabe0009e67214ff5f5447ce83dd", version = "v6.2.3", comment = "v6", repository = "actions/checkout", ref_type = "release", date = "2026-01-09T19:42:23Z" }
"#;
        let result = try_parse(content, Path::new("test.lock")).unwrap();
        assert!(result.is_some(), "v1.4 format must be parsed as flat");
        let lock = result.unwrap();
        let (res, commit) = lock.get(&make_key("actions/checkout", "^6")).unwrap();
        assert_eq!(res.version.as_str(), "v6.2.3");
        assert_eq!(res.comment, "v6");
        assert_eq!(
            commit.sha,
            CommitSha::from("de0fac2e4500dabe0009e67214ff5f5447ce83dd")
        );
    }

    #[test]
    fn flat_format_without_version_field_parses() {
        let content = r#"[actions."actions/checkout@^4"]
sha = "abc123def456789012345678901234567890abcd"
version = "v4.0.0"
comment = "v4"
repository = "actions/checkout"
ref_type = "tag"
date = "2026-01-01T00:00:00Z"
"#;
        let result = try_parse(content, Path::new("test.lock")).unwrap();
        assert!(result.is_some(), "flat format without version must parse");
        let lock = result.unwrap();
        let (res, commit) = lock.get(&make_key("actions/checkout", "^4")).unwrap();
        assert_eq!(res.version.as_str(), "v4.0.0");
        assert_eq!(
            commit.sha,
            CommitSha::from("abc123def456789012345678901234567890abcd")
        );
    }

    #[test]
    fn flat_entry_missing_version_falls_back_to_specifier() {
        let content = r#"[actions."actions/checkout@^4"]
sha = "abc123def456789012345678901234567890abcd"
comment = ""
repository = "actions/checkout"
ref_type = "tag"
date = ""
"#;
        let result = try_parse(content, Path::new("test.lock")).unwrap();
        assert!(result.is_some());
        let lock = result.unwrap();
        let (res, _) = lock.get(&make_key("actions/checkout", "^4")).unwrap();
        assert_eq!(
            res.version.as_str(),
            "^4",
            "missing version must fall back to specifier"
        );
    }

    #[test]
    fn two_flat_entries_deduplicating_to_one_action() {
        let content = r#"[actions."actions/checkout@^4"]
sha = "abc123def456789012345678901234567890abcd"
version = "v4.2.1"
comment = "v4"
repository = "actions/checkout"
ref_type = "tag"
date = "2026-01-01T00:00:00Z"

[actions."actions/checkout@^4.2"]
sha = "abc123def456789012345678901234567890abcd"
version = "v4.2.1"
comment = "v4.2"
repository = "actions/checkout"
ref_type = "tag"
date = "2026-01-01T00:00:00Z"
"#;
        let result = try_parse(content, Path::new("test.lock")).unwrap();
        assert!(result.is_some());
        let lock = result.unwrap();

        // Two resolution entries
        let spec1 = make_key("actions/checkout", "^4");
        let spec2 = make_key("actions/checkout", "^4.2");
        assert!(lock.has(&spec1));
        assert!(lock.has(&spec2));

        // But only one action entry (same version v4.2.1)
        let action_entries: Vec<_> = lock.action_entries().collect();
        let checkout_actions: Vec<_> = action_entries
            .iter()
            .filter(|(k, _)| k.id == ActionId::from("actions/checkout"))
            .collect();
        assert_eq!(
            checkout_actions.len(),
            1,
            "two flat entries with same version should deduplicate to one action entry"
        );
    }

    #[test]
    fn try_parse_returns_none_for_two_tier() {
        let content = r#"[resolutions."actions/checkout"."^4"]
version = "v4.0.0"
comment = "v4"

[actions."actions/checkout"."v4.0.0"]
sha = "abc123"
repository = "actions/checkout"
ref_type = "tag"
date = ""
"#;
        let result = try_parse(content, Path::new("test.lock")).unwrap();
        assert!(result.is_none(), "two-tier format should return None");
    }

    #[test]
    fn try_parse_returns_none_for_no_actions() {
        let content = "# empty lock file\n";
        let result = try_parse(content, Path::new("test.lock")).unwrap();
        assert!(
            result.is_none(),
            "content without [actions] should return None"
        );
    }
}

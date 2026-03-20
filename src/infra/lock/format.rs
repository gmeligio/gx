use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use crate::domain::action::resolved::Commit;
use crate::domain::action::spec::Spec;
use crate::domain::action::specifier::Specifier;
use crate::domain::action::uses_ref::RefType;
use crate::domain::lock::{Lock, LockEntry};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use toml_edit::DocumentMut;

/// Resolution entry in the two-tier format.
#[derive(Debug, Clone, Deserialize)]
pub struct ResolutionEntryData {
    /// The resolved version string (e.g. "v4.2.1").
    pub version: String,
}

/// Action commit entry in the two-tier format.
#[derive(Debug, Clone, Deserialize)]
pub struct ActionCommitData {
    /// The full commit SHA.
    pub sha: String,
    /// The source repository (e.g. "actions/checkout").
    pub repository: String,
    /// The ref type that was resolved.
    pub ref_type: String,
    /// ISO 8601 date of the resolved commit or release.
    pub date: String,
}

/// Internal structure for two-tier TOML deserialization.
#[derive(Debug, Deserialize, Default)]
pub struct TwoTierData {
    /// Map of action ID -> specifier -> resolution data.
    #[serde(default)]
    pub resolutions: HashMap<String, HashMap<String, ResolutionEntryData>>,
    /// Map of action ID -> version -> commit data.
    #[serde(default)]
    pub actions: HashMap<String, HashMap<String, ActionCommitData>>,
}

/// Try to parse lock file content as the current two-tier format.
///
/// Returns `Ok(Some(lock))` if the content is two-tier format (contains `[resolutions`),
/// `Ok(None)` if the content is not two-tier format, or `Err` if parsing fails.
pub fn try_parse(content: &str, path: &Path) -> Result<Option<Lock>, super::Error> {
    if !content.contains("[resolutions") {
        return Ok(None);
    }

    let data: TwoTierData = super::parse_toml(content, path)?;
    Ok(Some(lock_from_two_tier(&data)))
}

/// Convert deserialized two-tier lock data into a domain `Lock`.
fn lock_from_two_tier(data: &TwoTierData) -> Lock {
    let mut entries = HashMap::new();

    // First, index the action commit data by (action_id, version) for lookup.
    let mut action_index: HashMap<(&str, &str), &ActionCommitData> = HashMap::new();
    for (action_id, versions) in &data.actions {
        for (version, commit_data) in versions {
            action_index.insert((action_id.as_str(), version.as_str()), commit_data);
        }
    }

    for (action_id, specifiers) in &data.resolutions {
        for (specifier, res_data) in specifiers {
            let spec = Spec::new(
                ActionId::from(action_id.as_str()),
                Specifier::parse(specifier),
            );
            let version = Version::from(res_data.version.as_str());

            // Look up the corresponding action commit data.
            if let Some(commit_data) =
                action_index.get(&(action_id.as_str(), res_data.version.as_str()))
            {
                entries.insert(
                    spec,
                    LockEntry {
                        version,
                        commit: Commit {
                            sha: CommitSha::from(commit_data.sha.as_str()),
                            repository: Repository::from(commit_data.repository.as_str()),
                            ref_type: RefType::parse(&commit_data.ref_type),
                            date: CommitDate::from(commit_data.date.as_str()),
                        },
                    },
                );
            }
        }
    }

    Lock::new(entries)
}

/// Serialize a `Lock` to the two-tier TOML format string.
pub(super) fn write(lock: &Lock) -> String {
    build_lock_document(lock).to_string()
}

/// Build a `toml_edit::DocumentMut` from a `Lock` using the two-tier format.
///
/// Writes `[resolutions]` and `[actions]` sections with nested TOML tables.
/// Resolutions are sorted by action ID then specifier.
/// Actions are sorted by action ID then version.
/// No top-level `version` field is written.
fn build_lock_document(lock: &Lock) -> DocumentMut {
    let mut doc = DocumentMut::new();

    // Collect entries sorted by action ID then specifier.
    let mut sorted_entries: Vec<_> = lock.entries().collect();
    sorted_entries.sort_by(|(a, _), (b, _)| {
        a.id.as_str()
            .cmp(b.id.as_str())
            .then_with(|| a.specifier.as_str().cmp(b.specifier.as_str()))
    });

    // --- [resolutions] tier ---
    let mut resolutions = toml_edit::Table::new();
    resolutions.set_implicit(true);

    for (spec, entry) in &sorted_entries {
        let id_str = spec.id.as_str();
        let specifier_str = spec.specifier.as_str();

        ensure_implicit_table(&mut resolutions, id_str);

        let Some(id_table) = resolutions
            .get_mut(id_str)
            .and_then(toml_edit::Item::as_table_mut)
        else {
            continue;
        };

        let mut entry_table = toml_edit::Table::new();
        entry_table.insert("version", toml_edit::value(entry.version.as_str()));
        id_table.insert(specifier_str, toml_edit::Item::Table(entry_table));
    }

    doc.insert("resolutions", toml_edit::Item::Table(resolutions));

    // --- [actions] tier ---
    // Deduplicate by (action_id, version) since multiple specs can share the same commit.
    let mut actions = toml_edit::Table::new();
    actions.set_implicit(true);

    // Collect unique (id, version) -> commit, sorted.
    let mut action_map: HashMap<(&str, &str), &Commit> = HashMap::new();
    for (spec, entry) in &sorted_entries {
        action_map
            .entry((spec.id.as_str(), entry.version.as_str()))
            .or_insert(&entry.commit);
    }

    let mut action_keys: Vec<_> = action_map.keys().copied().collect();
    action_keys
        .sort_by(|(a_id, a_ver), (b_id, b_ver)| a_id.cmp(b_id).then_with(|| a_ver.cmp(b_ver)));

    for (id_str, version_str) in &action_keys {
        let Some(commit) = action_map.get(&(*id_str, *version_str)) else {
            continue;
        };

        ensure_implicit_table(&mut actions, id_str);

        let Some(id_table) = actions
            .get_mut(id_str)
            .and_then(toml_edit::Item::as_table_mut)
        else {
            continue;
        };

        let mut entry_table = toml_edit::Table::new();
        populate_action_table(&mut entry_table, commit);
        id_table.insert(version_str, toml_edit::Item::Table(entry_table));
    }

    doc.insert("actions", toml_edit::Item::Table(actions));
    doc
}

/// Ensure a nested implicit table exists at the given key.
fn ensure_implicit_table(parent: &mut toml_edit::Table, key: &str) {
    if parent.get(key).is_none() {
        let mut table = toml_edit::Table::new();
        table.set_implicit(true);
        parent.insert(key, toml_edit::Item::Table(table));
    }
}

/// Convert a `RefType` option to its string representation.
fn ref_type_to_str(ref_type: Option<&RefType>) -> &'static str {
    ref_type.map_or("unknown", |r| match r {
        RefType::Release => "release",
        RefType::Tag => "tag",
        RefType::Branch => "branch",
        RefType::Commit => "commit",
    })
}

/// Populate a TOML table with action commit metadata (4 fields).
fn populate_action_table(
    table: &mut toml_edit::Table,
    commit: &crate::domain::action::resolved::Commit,
) {
    table.insert("sha", toml_edit::value(commit.sha.as_str()));
    table.insert("repository", toml_edit::value(commit.repository.as_str()));
    table.insert(
        "ref_type",
        toml_edit::value(ref_type_to_str(commit.ref_type.as_ref())),
    );
    table.insert("date", toml_edit::value(commit.date.as_str()));
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::*;
    use crate::domain::action::identity::{ActionId, CommitSha};
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;

    fn set_resolved(lock: &mut Lock, action: &str, specifier: &str, sha: &str) {
        let spec = Spec::new(ActionId::from(action), Specifier::parse(specifier));
        let version = Version::from(Specifier::parse(specifier).to_lookup_tag());
        lock.set(
            &spec,
            version,
            Commit {
                sha: CommitSha::from(sha),
                repository: ActionId::from(action).base_repo(),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
    }

    #[test]
    fn roundtrip_write_then_parse() {
        let mut lock = Lock::default();
        set_resolved(
            &mut lock,
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcd",
        );
        set_resolved(
            &mut lock,
            "actions/setup-node",
            "^3",
            "def456789012345678901234567890abcdef1234",
        );

        let output = write(&lock);
        let parsed = try_parse(&output, Path::new("test.lock"))
            .unwrap()
            .expect("should parse as two-tier");

        // Verify all entries survived the roundtrip
        let spec1 = Spec::new(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        let spec2 = Spec::new(ActionId::from("actions/setup-node"), Specifier::parse("^3"));

        assert!(parsed.has(&spec1), "checkout entry must survive roundtrip");
        assert!(
            parsed.has(&spec2),
            "setup-node entry must survive roundtrip"
        );

        let entry1 = parsed.get(&spec1).unwrap();
        assert_eq!(
            entry1.commit.sha,
            CommitSha::from("abc123def456789012345678901234567890abcd")
        );

        let entry2 = parsed.get(&spec2).unwrap();
        assert_eq!(
            entry2.commit.sha,
            CommitSha::from("def456789012345678901234567890abcdef1234")
        );
    }

    #[test]
    fn try_parse_returns_none_for_non_two_tier() {
        let content = r#"version = "1.4"

[actions]
"actions/checkout@^4" = { sha = "abc123", version = "v4.0.0", comment = "v4", repository = "actions/checkout", ref_type = "tag", date = "" }
"#;
        let result = try_parse(content, Path::new("test.lock")).unwrap();
        assert!(result.is_none(), "flat format should return None");
    }

    #[test]
    fn write_produces_sorted_output() {
        let mut lock = Lock::default();
        set_resolved(
            &mut lock,
            "docker/build-push-action",
            "^5",
            "def456789012345678901234567890abcdef123456",
        );
        set_resolved(
            &mut lock,
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcdef",
        );

        let output = write(&lock);
        let checkout_pos = output.find("actions/checkout").unwrap();
        let docker_pos = output.find("docker/build-push-action").unwrap();
        assert!(
            checkout_pos < docker_pos,
            "entries must be sorted alphabetically"
        );
    }
}

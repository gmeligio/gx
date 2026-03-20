use super::Store;
use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use crate::domain::action::resolved::Commit;
use crate::domain::action::spec::Spec;
use crate::domain::action::specifier::Specifier;
use crate::domain::action::uses_ref::RefType;
use std::io::Write as _;
use std::path::Path;
use tempfile::NamedTempFile;

fn make_key(action: &str, specifier: &str) -> Spec {
    Spec::new(ActionId::from(action), Specifier::parse(specifier))
}

fn set_resolved(lock: &mut crate::domain::lock::Lock, action: &str, specifier: &str, sha: &str) {
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

/// Helper to build two-tier lock file content (resolutions + actions, no version field).
///
/// `key` is in `"owner/repo@specifier"` format (e.g. `"actions/checkout@^4"`).
fn two_tier_entry(
    key: &str,
    sha: &str,
    version: &str,
    repository: &str,
    ref_type: &str,
    date: &str,
) -> String {
    let (action_id, specifier) = key.rsplit_once('@').unwrap();
    format!(
        "[resolutions.\"{action_id}\".\"{specifier}\"]\nversion = \"{version}\"\n\n[actions.\"{action_id}\".\"{version}\"]\nsha = \"{sha}\"\nrepository = \"{repository}\"\nref_type = \"{ref_type}\"\ndate = \"{date}\"\n"
    )
}

// ========== Store::load tests ==========

#[test]
fn load_file_does_not_exist_returns_default() {
    let store = Store::new(Path::new("/nonexistent/gx.lock"));
    let lock = store.load().unwrap();
    assert!(lock.is_empty());
}

#[test]
fn load_file_empty_returns_default() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(b"").unwrap();
    let store = Store::new(file.path());
    let lock = store.load().unwrap();
    assert!(lock.is_empty());
}

#[test]
fn load_two_tier_format() {
    let content = two_tier_entry(
        "actions/checkout@^4",
        "abc123def456789012345678901234567890abcd",
        "v4.0.0",
        "actions/checkout",
        "tag",
        "2026-01-01T00:00:00Z",
    );
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let store = Store::new(file.path());
    let lock = store.load().unwrap();
    assert!(lock.has(&make_key("actions/checkout", "^4")));
    let entry = lock.get(&make_key("actions/checkout", "^4")).unwrap();
    assert_eq!(
        entry.commit.sha,
        CommitSha::from("abc123def456789012345678901234567890abcd")
    );
    assert_eq!(entry.version.as_str(), "v4.0.0");
}

#[test]
fn load_flat_format() {
    let content = r#"version = "1.4"

[actions]
"actions/checkout@^6" = { sha = "de0fac2e4500dabe0009e67214ff5f5447ce83dd", version = "v6.2.3", comment = "v6", repository = "actions/checkout", ref_type = "release", date = "2026-01-09T19:42:23Z" }
"#;
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let store = Store::new(file.path());
    let lock = store.load().unwrap();
    let entry = lock.get(&make_key("actions/checkout", "^6")).unwrap();
    assert_eq!(entry.version.as_str(), "v6.2.3");
    assert_eq!(
        entry.commit.sha,
        CommitSha::from("de0fac2e4500dabe0009e67214ff5f5447ce83dd")
    );
}

#[test]
fn load_unrecognized_content_returns_error() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(b"this is not a lock file").unwrap();

    let store = Store::new(file.path());
    let result = store.load();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("unrecognized"),
        "error should mention unrecognized format, got: {err}"
    );
}

// ========== Store::save tests ==========

#[test]
fn save_and_load_roundtrip() {
    let file = NamedTempFile::new().unwrap();
    let store = Store::new(file.path());

    let mut lock = crate::domain::lock::Lock::default();
    set_resolved(
        &mut lock,
        "actions/checkout",
        "^4",
        "abc123def456789012345678901234567890abcd",
    );

    store.save(&lock).unwrap();

    let loaded = store.load().unwrap();
    let result = loaded.get(&make_key("actions/checkout", "^4"));
    assert!(result.is_some());
    let entry = result.unwrap();
    assert_eq!(
        entry.commit.sha,
        CommitSha::from("abc123def456789012345678901234567890abcd")
    );
}

#[test]
fn save_sorts_actions_alphabetically() {
    let file = NamedTempFile::new().unwrap();
    let store = Store::new(file.path());

    let mut lock = crate::domain::lock::Lock::default();
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
    set_resolved(
        &mut lock,
        "actions-rust-lang/rustfmt",
        "^1",
        "111222333444555666777888999000aaabbbcccddd",
    );

    store.save(&lock).unwrap();

    let content = std::fs::read_to_string(file.path()).unwrap();
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
fn save_produces_two_tier_format() {
    let file = NamedTempFile::new().unwrap();
    let store = Store::new(file.path());

    let mut lock = crate::domain::lock::Lock::default();
    set_resolved(
        &mut lock,
        "actions/checkout",
        "^4",
        "abc123def456789012345678901234567890abcd",
    );

    store.save(&lock).unwrap();

    let content = std::fs::read_to_string(file.path()).unwrap();
    assert!(!content.contains("version = \"1.4\""));
    assert!(content.contains("[resolutions.\"actions/checkout\".\"^4\"]"));
    assert!(content.contains("[actions.\"actions/checkout\""));
    assert!(!content.contains("{ sha ="), "entries must NOT be inline");
}

#[test]
fn save_roundtrip_preserves_all_fields() {
    let file = NamedTempFile::new().unwrap();
    let store = Store::new(file.path());

    let mut lock = crate::domain::lock::Lock::default();
    let spec = make_key("actions/checkout", "^4");
    let version = Version::from("v4.2.0");
    let commit = Commit {
        sha: CommitSha::from("abc123def456789012345678901234567890abcd"),
        repository: Repository::from("actions/checkout"),
        ref_type: Some(RefType::Release),
        date: CommitDate::from("2026-01-15T10:30:00Z"),
    };
    lock.set(&spec, version.clone(), commit.clone());

    store.save(&lock).unwrap();
    let loaded = store.load().unwrap();

    let loaded_entry = loaded.get(&spec).expect("Entry must exist");
    assert_eq!(loaded_entry.commit.sha, commit.sha);
    assert_eq!(loaded_entry.version.as_str(), version.as_str());
    assert_eq!(
        loaded_entry.commit.repository.as_str(),
        commit.repository.as_str()
    );
    assert_eq!(loaded_entry.commit.ref_type, commit.ref_type);
    assert_eq!(loaded_entry.commit.date.as_str(), commit.date.as_str());
}

use super::Store;
use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use crate::domain::action::resolved::{Commit, ResolvedRef};
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
        ResolvedRef::Tag(version),
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
    assert_eq!(entry.version_label(), "v4.0.0");
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
    assert_eq!(entry.version_label(), "v6.2.3");
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

/// A canonical two-tier lock covering every ref kind gx writes: a semver tag,
/// two specifiers sharing one commit, a branch pin, and a bare-commit pin.
///
/// Bare TOML keys (`main`, the SHA) are unquoted because that is what the
/// document builder emits.
const CANONICAL_LOCK: &str = concat!(
    "[resolutions.\"actions/checkout\".\"^4\"]\nversion = \"v4.2.1\"\n\n",
    "[resolutions.\"actions/checkout\".\"^4.2\"]\nversion = \"v4.2.1\"\n\n",
    "[resolutions.\"actions/setup-node\".main]\nversion = \"main\"\n\n",
    "[resolutions.\"docker/build-push-action\".0011223344556677889900aabbccddeeff001122]\nversion = \"0011223344556677889900aabbccddeeff001122\"\n\n",
    "[actions.\"actions/checkout\".\"v4.2.1\"]\nsha = \"abc123def456789012345678901234567890abcd\"\nrepository = \"actions/checkout\"\nref_type = \"tag\"\ndate = \"2026-01-01T00:00:00Z\"\n\n",
    "[actions.\"actions/setup-node\".main]\nsha = \"def456789012345678901234567890abcdef1234\"\nrepository = \"actions/setup-node\"\nref_type = \"branch\"\ndate = \"2026-02-02T00:00:00Z\"\n\n",
    "[actions.\"docker/build-push-action\".0011223344556677889900aabbccddeeff001122]\nsha = \"0011223344556677889900aabbccddeeff001122\"\nrepository = \"docker/build-push-action\"\nref_type = \"commit\"\ndate = \"2026-03-03T00:00:00Z\"\n",
);

/// Load `content` through a `Store`, save it straight back, and return the bytes written.
fn load_then_save(content: &str) -> String {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    let store = Store::new(file.path());
    let lock = store.load().unwrap();
    store.save(&lock).unwrap();
    std::fs::read_to_string(file.path()).unwrap()
}

#[test]
fn two_tier_lock_roundtrips_byte_identically() {
    assert_eq!(
        load_then_save(CANONICAL_LOCK),
        CANONICAL_LOCK,
        "reading and rewriting an unchanged lock must not alter a single byte"
    );
}

#[test]
fn save_sorts_unsorted_input_into_canonical_order() {
    // `Lock`'s `HashMap` discards this order before the serializer sees it; the
    // assertion holds for whatever order iteration yields.
    let unsorted = concat!(
        "[resolutions.\"docker/build-push-action\".\"^5\"]\nversion = \"v5.1.0\"\n\n",
        "[resolutions.\"actions/checkout\".\"^4.2\"]\nversion = \"v4.2.1\"\n\n",
        "[resolutions.\"actions/checkout\".\"^4\"]\nversion = \"v4.2.1\"\n\n",
        "[actions.\"docker/build-push-action\".\"v5.1.0\"]\nsha = \"1111111111111111111111111111111111111111\"\nrepository = \"docker/build-push-action\"\nref_type = \"tag\"\ndate = \"2026-03-03T00:00:00Z\"\n\n",
        "[actions.\"actions/checkout\".\"v4.2.1\"]\nsha = \"2222222222222222222222222222222222222222\"\nrepository = \"actions/checkout\"\nref_type = \"tag\"\ndate = \"2026-01-01T00:00:00Z\"\n",
    );
    let expected = concat!(
        "[resolutions.\"actions/checkout\".\"^4\"]\nversion = \"v4.2.1\"\n\n",
        "[resolutions.\"actions/checkout\".\"^4.2\"]\nversion = \"v4.2.1\"\n\n",
        "[resolutions.\"docker/build-push-action\".\"^5\"]\nversion = \"v5.1.0\"\n\n",
        "[actions.\"actions/checkout\".\"v4.2.1\"]\nsha = \"2222222222222222222222222222222222222222\"\nrepository = \"actions/checkout\"\nref_type = \"tag\"\ndate = \"2026-01-01T00:00:00Z\"\n\n",
        "[actions.\"docker/build-push-action\".\"v5.1.0\"]\nsha = \"1111111111111111111111111111111111111111\"\nrepository = \"docker/build-push-action\"\nref_type = \"tag\"\ndate = \"2026-03-03T00:00:00Z\"\n",
    );

    assert_ne!(
        unsorted, expected,
        "expected output must be the sorted form, not a copy of the fixture"
    );
    assert_eq!(
        load_then_save(unsorted),
        expected,
        "entries must be sorted by action ID then specifier/version"
    );
}

#[test]
fn two_specs_sharing_a_version_write_the_first_commit_in_sort_order() {
    // Built in memory, not from a fixture: two specs loaded from disk share one
    // `[actions]` row, so their commits can never disagree to begin with.
    let mut lock = crate::domain::lock::Lock::default();
    for (specifier, sha) in [
        ("^4.2", "2222222222222222222222222222222222222222"),
        ("^4", "1111111111111111111111111111111111111111"),
    ] {
        lock.set(
            &make_key("actions/checkout", specifier),
            ResolvedRef::Tag(Version::from("v4.2.1")),
            Commit {
                sha: CommitSha::from(sha),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
    }

    let file = NamedTempFile::new().unwrap();
    let store = Store::new(file.path());
    store.save(&lock).unwrap();
    let content = std::fs::read_to_string(file.path()).unwrap();

    assert!(
        content.contains("sha = \"1111111111111111111111111111111111111111\""),
        "`^4` sorts before `^4.2`, so its commit wins the dedup:\n{content}"
    );
    assert!(
        !content.contains("2222222222222222222222222222222222222222"),
        "the later spec's commit must not overwrite the first:\n{content}"
    );
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
    lock.set(&spec, ResolvedRef::Tag(version.clone()), commit.clone());

    store.save(&lock).unwrap();
    let loaded = store.load().unwrap();

    let loaded_entry = loaded.get(&spec).expect("Entry must exist");
    assert_eq!(loaded_entry.commit.sha, commit.sha);
    assert_eq!(loaded_entry.version_label(), version.as_str());
    assert_eq!(
        loaded_entry.commit.repository.as_str(),
        commit.repository.as_str()
    );
    assert_eq!(loaded_entry.commit.ref_type, commit.ref_type);
    assert_eq!(loaded_entry.commit.date.as_str(), commit.date.as_str());
}

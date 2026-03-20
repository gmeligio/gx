use super::Lock;
use crate::domain::action::identity::ActionId;
use crate::domain::action::identity::{CommitDate, CommitSha, Repository, Version};
use crate::domain::action::resolved::Commit;
use crate::domain::action::spec::Spec;
use crate::domain::action::specifier::Specifier;
use crate::domain::action::uses_ref::RefType;

fn make_key(action: &str, specifier: &str) -> Spec {
    Spec::new(ActionId::from(action), Specifier::parse(specifier))
}

fn make_commit(sha: &str) -> Commit {
    Commit {
        sha: CommitSha::from(sha),
        repository: Repository::from("actions/checkout"),
        ref_type: Some(RefType::Tag),
        date: CommitDate::from("2026-01-01T00:00:00Z"),
    }
}

fn set_action(lock: &mut Lock, action: &str, specifier: &str, sha: &str, version: &str) {
    let spec = make_key(action, specifier);
    let ver = Version::from(version);
    lock.set(&spec, ver, make_commit(sha));
}

#[test]
fn new_empty() {
    let lock = Lock::default();
    assert!(lock.get(&make_key("actions/checkout", "^4")).is_none());
}

#[test]
fn set_and_get() {
    let mut lock = Lock::default();
    set_action(
        &mut lock,
        "actions/checkout",
        "^4",
        "abc123def456789012345678901234567890abcd",
        "v4.2.1",
    );
    let result = lock.get(&make_key("actions/checkout", "^4"));
    assert!(result.is_some());
    let entry = result.unwrap();
    assert_eq!(
        entry.commit.sha,
        CommitSha::from("abc123def456789012345678901234567890abcd")
    );
    assert_eq!(entry.version, Version::from("v4.2.1"));
    assert!(lock.get(&make_key("actions/checkout", "^3")).is_none());
}

#[test]
fn has() {
    let mut lock = Lock::default();
    set_action(
        &mut lock,
        "actions/checkout",
        "^4",
        "abc123def456789012345678901234567890abcd",
        "v4.2.1",
    );
    assert!(lock.has(&make_key("actions/checkout", "^4")));
    assert!(!lock.has(&make_key("actions/checkout", "^3")));
}

#[test]
fn retain() {
    let mut lock = Lock::default();
    set_action(
        &mut lock,
        "actions/checkout",
        "^4",
        "abc123def456789012345678901234567890abcd",
        "v4.2.1",
    );
    set_action(
        &mut lock,
        "actions/setup-node",
        "^3",
        "def456789012345678901234567890abcd123456",
        "v3.1.0",
    );
    set_action(
        &mut lock,
        "actions/old-action",
        "^1",
        "xyz789012345678901234567890abcd12345678a",
        "v1.0.0",
    );

    let keep = vec![
        make_key("actions/checkout", "^4"),
        make_key("actions/setup-node", "^3"),
    ];
    lock.retain(&keep);

    assert!(lock.has(&make_key("actions/checkout", "^4")));
    assert!(lock.has(&make_key("actions/setup-node", "^3")));
    assert!(!lock.has(&make_key("actions/old-action", "^1")));
}

#[test]
fn update_existing_sha() {
    let mut lock = Lock::default();
    set_action(
        &mut lock,
        "actions/checkout",
        "^4",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "v4.2.1",
    );
    set_action(
        &mut lock,
        "actions/checkout",
        "^4",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "v4.2.1",
    );
    let result = lock.get(&make_key("actions/checkout", "^4"));
    assert!(result.is_some());
    let entry = result.unwrap();
    assert_eq!(
        entry.commit.sha,
        CommitSha::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    );
}

#[test]
fn is_complete_all_fields() {
    let mut lock = Lock::default();
    set_action(
        &mut lock,
        "actions/checkout",
        "^4",
        "abc123def456789012345678901234567890abcd",
        "v4.0.0",
    );
    assert!(lock.is_complete(&make_key("actions/checkout", "^4")));
}

#[test]
fn is_complete_missing_resolution() {
    let lock = Lock::default();
    assert!(!lock.is_complete(&make_key("actions/checkout", "^4")));
}

#[test]
fn is_complete_non_semver_ref() {
    let mut lock = Lock::default();
    let spec = make_key("actions/checkout", "main");
    lock.set(
        &spec,
        Version::from("main"),
        Commit {
            sha: CommitSha::from("abc123def456789012345678901234567890abcd"),
            repository: Repository::from("actions/checkout"),
            ref_type: Some(RefType::Branch),
            date: CommitDate::from("2026-01-01T00:00:00Z"),
        },
    );
    assert!(lock.is_complete(&spec));
}

#[test]
fn set_version_updates_entry() {
    let mut lock = Lock::default();
    set_action(
        &mut lock,
        "actions/checkout",
        "^4",
        "abc123def456789012345678901234567890abcd",
        "v4",
    );
    let spec = make_key("actions/checkout", "^4");
    lock.set_version(&spec, Some("v4.2.1".to_owned()));

    let entry = lock.get(&spec).unwrap();
    assert_eq!(entry.version, Version::from("v4.2.1"));
    assert_eq!(
        entry.commit.sha,
        CommitSha::from("abc123def456789012345678901234567890abcd")
    );
}

// --- Lock::diff tests ---

#[test]
fn lock_diff_empty_locks_is_empty() {
    let before = Lock::default();
    let after = Lock::default();
    assert!(before.diff(&after).is_empty());
}

#[test]
fn lock_diff_detects_added_entry() {
    let before = Lock::default();
    let mut after = Lock::default();
    set_action(
        &mut after,
        "actions/checkout",
        "^4",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "v4.0.0",
    );

    let diff = before.diff(&after);
    assert_eq!(diff.added.len(), 1);
    assert_eq!(diff.added[0].0, make_key("actions/checkout", "^4"));
    assert!(diff.removed.is_empty());
}

#[test]
fn lock_diff_detects_removed_entry() {
    let mut before = Lock::default();
    set_action(
        &mut before,
        "actions/checkout",
        "^4",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "v4.0.0",
    );
    let after = Lock::default();

    let diff = before.diff(&after);
    assert!(diff.added.is_empty());
    assert_eq!(diff.removed.len(), 1);
    assert_eq!(diff.removed[0], make_key("actions/checkout", "^4"));
}

#[test]
fn lock_diff_same_sha_not_in_diff() {
    let mut before = Lock::default();
    set_action(
        &mut before,
        "actions/checkout",
        "^4",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "v4.0.0",
    );
    let after = before.clone();

    let diff = before.diff(&after);
    assert!(diff.is_empty());
}

#[test]
fn lock_diff_sha_replaced_appears_in_both_added_and_removed() {
    let mut before = Lock::default();
    set_action(
        &mut before,
        "actions/checkout",
        "^4",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "v4.0.0",
    );
    let mut after = Lock::default();
    set_action(
        &mut after,
        "actions/checkout",
        "^4",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "v4.0.0",
    );

    let diff = before.diff(&after);
    assert_eq!(diff.added.len(), 1, "replaced entry should appear in added");
    assert_eq!(
        diff.removed.len(),
        1,
        "replaced entry should appear in removed"
    );
}

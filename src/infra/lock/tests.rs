use super::{Error as LockFileError, LOCK_FILE_VERSION, apply_lock_diff, create, parse};
use crate::domain::action::identity::{ActionId, CommitSha};
use crate::domain::action::spec::LockKey;
use crate::domain::action::specifier::Specifier;
use crate::domain::action::uses_ref::RefType;
use crate::domain::lock::entry::Entry as LockEntry;
use crate::domain::plan::LockDiff;
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

fn make_key(action: &str, specifier: &str) -> LockKey {
    LockKey::new(ActionId::from(action), Specifier::parse(specifier))
}

#[test]
fn test_parse_unknown_version_errors() {
    let content = r#"version = "2.0"

[actions]
"#;
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let result = parse(file.path());
    assert!(matches!(result, Err(LockFileError::UnsupportedVersion(_))));
}

#[test]
fn test_parse_missing_returns_empty() {
    let parsed = parse(Path::new("/nonexistent/gx.lock")).unwrap();
    assert!(!parsed.value.has(&make_key("actions/checkout", "^4")));
    assert!(!parsed.migrated);
}

#[test]
fn test_parse_reads_file() {
    let content = format!(
        "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@^4\" = {{ sha = \"abc123\", version = \"v4.0.0\", comment = \"v4\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"\" }}\n"
    );
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    let parsed = parse(file.path()).unwrap();
    assert!(parsed.value.has(&make_key("actions/checkout", "^4")));
}

// ========== apply_lock_diff tests ==========

#[test]
fn test_apply_lock_empty_diff_does_not_modify_file() {
    let content = format!(
        "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@^4\" = {{ sha = \"abc123def456789012345678901234567890abcd\", version = \"v4.0.0\", comment = \"v4\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"2026-01-01T00:00:00Z\" }}\n"
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
        "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@^4\" = {{ sha = \"abc123def456789012345678901234567890abcd\", version = \"v4.0.0\", comment = \"v4\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"2026-01-01T00:00:00Z\" }}\n"
    );
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let diff = LockDiff {
        added: vec![(
            make_key("actions/setup-node", "^3"),
            LockEntry::with_version_and_comment(
                CommitSha::from("def456789012345678901234567890abcdef1234"),
                Some("v3.1.0".to_owned()),
                "v3".to_owned(),
                "actions/setup-node".to_owned(),
                Some(RefType::Tag),
                "2026-01-01T00:00:00Z".to_owned(),
            ),
        )],
        ..Default::default()
    };
    apply_lock_diff(file.path(), &diff).unwrap();

    // Round-trip
    let loaded = parse(file.path()).unwrap();
    assert!(
        loaded.value.has(&make_key("actions/checkout", "^4")),
        "Existing entry preserved"
    );
    let entry = loaded
        .value
        .get(&make_key("actions/setup-node", "^3"))
        .expect("New entry exists");
    assert_eq!(
        entry.sha,
        CommitSha::from("def456789012345678901234567890abcdef1234")
    );
    assert_eq!(entry.version, Some("v3.1.0".to_owned()));
    assert_eq!(entry.comment, "v3");
    assert_eq!(entry.repository, "actions/setup-node");
}

#[test]
fn test_apply_lock_remove_one_entry() {
    let content = format!(
        "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@^4\" = {{ sha = \"abc123\", version = \"v4\", comment = \"\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"\" }}\n\"actions/setup-node@^3\" = {{ sha = \"def456\", version = \"v3\", comment = \"\", repository = \"actions/setup-node\", ref_type = \"tag\", date = \"\" }}\n"
    );
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let diff = LockDiff {
        removed: vec![make_key("actions/checkout", "^4")],
        ..Default::default()
    };
    apply_lock_diff(file.path(), &diff).unwrap();

    let loaded = parse(file.path()).unwrap();
    assert!(!loaded.value.has(&make_key("actions/checkout", "^4")));
    assert!(loaded.value.has(&make_key("actions/setup-node", "^3")));
}

#[test]
fn test_apply_lock_update_version_field() {
    let content = format!(
        "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@^4\" = {{ sha = \"abc123\", version = \"v4\", comment = \"v4\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"2026-01-01T00:00:00Z\" }}\n"
    );
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let diff = LockDiff {
        updated: vec![(
            make_key("actions/checkout", "^4"),
            crate::domain::plan::LockEntryPatch {
                version: Some(Some("v4.1.0".to_owned())),
                comment: None,
            },
        )],
        ..Default::default()
    };
    apply_lock_diff(file.path(), &diff).unwrap();

    let loaded = parse(file.path()).unwrap();
    let entry = loaded
        .value
        .get(&make_key("actions/checkout", "^4"))
        .unwrap();
    assert_eq!(entry.version, Some("v4.1.0".to_owned()));
    assert_eq!(entry.comment, "v4", "Comment must be unchanged");
}

#[test]
fn test_apply_lock_update_comment_field() {
    let content = format!(
        "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@^4\" = {{ sha = \"abc123\", version = \"v4\", comment = \"v4\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"2026-01-01T00:00:00Z\" }}\n"
    );
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let diff = LockDiff {
        updated: vec![(
            make_key("actions/checkout", "^4"),
            crate::domain::plan::LockEntryPatch {
                version: None,
                comment: Some("v4.1".to_owned()),
            },
        )],
        ..Default::default()
    };
    apply_lock_diff(file.path(), &diff).unwrap();

    let loaded = parse(file.path()).unwrap();
    let entry = loaded
        .value
        .get(&make_key("actions/checkout", "^4"))
        .unwrap();
    assert_eq!(
        entry.version,
        Some("v4".to_owned()),
        "Version must be unchanged"
    );
    assert_eq!(entry.comment, "v4.1");
}

#[test]
fn test_apply_lock_roundtrip() {
    let content = format!(
        "version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n\"actions/checkout@^4\" = {{ sha = \"abc123def456789012345678901234567890abcd\", version = \"v4\", comment = \"v4\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"2026-01-01T00:00:00Z\" }}\n\"actions/old-action@^1\" = {{ sha = \"aaabbbcccdddeeefffaaabbbcccdddeeefffaaab\", version = \"v1\", comment = \"\", repository = \"actions/old-action\", ref_type = \"tag\", date = \"\" }}\n"
    );
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let diff = LockDiff {
        added: vec![(
            make_key("actions/setup-node", "^3"),
            LockEntry::with_version_and_comment(
                CommitSha::from("def456789012345678901234567890abcdef1234"),
                Some("v3.1.0".to_owned()),
                "v3".to_owned(),
                "actions/setup-node".to_owned(),
                Some(RefType::Tag),
                "2026-01-01T00:00:00Z".to_owned(),
            ),
        )],
        removed: vec![make_key("actions/old-action", "^1")],
        updated: vec![],
    };
    apply_lock_diff(file.path(), &diff).unwrap();

    let loaded = parse(file.path()).unwrap();
    assert!(loaded.value.has(&make_key("actions/checkout", "^4")));
    assert!(loaded.value.has(&make_key("actions/setup-node", "^3")));
    assert!(!loaded.value.has(&make_key("actions/old-action", "^1")));
}

// ========== create tests ==========

#[test]
fn test_create_from_diff_with_3_entries() {
    let file = NamedTempFile::new().unwrap();

    let diff = LockDiff {
        added: vec![
            (
                make_key("actions/checkout", "^4"),
                LockEntry::with_version_and_comment(
                    CommitSha::from("abc123def456789012345678901234567890abcd"),
                    Some("v4.1.0".to_owned()),
                    "v4".to_owned(),
                    "actions/checkout".to_owned(),
                    Some(RefType::Tag),
                    "2026-01-01T00:00:00Z".to_owned(),
                ),
            ),
            (
                make_key("actions/setup-node", "^3"),
                LockEntry::with_version_and_comment(
                    CommitSha::from("def456789012345678901234567890abcdef1234"),
                    Some("v3.2.0".to_owned()),
                    "v3".to_owned(),
                    "actions/setup-node".to_owned(),
                    Some(RefType::Tag),
                    "2026-01-01T00:00:00Z".to_owned(),
                ),
            ),
            (
                make_key("actions/cache", "^3"),
                LockEntry::with_version_and_comment(
                    CommitSha::from("111222333444555666777888999000aaabbbcccddd"),
                    Some("v3.0.0".to_owned()),
                    "v3".to_owned(),
                    "actions/cache".to_owned(),
                    Some(RefType::Tag),
                    "2026-01-01T00:00:00Z".to_owned(),
                ),
            ),
        ],
        ..Default::default()
    };
    create(file.path(), &diff).unwrap();

    let content = fs::read_to_string(file.path()).unwrap();
    assert!(content.contains(&format!("version = \"{LOCK_FILE_VERSION}\"")));
    assert!(content.contains("[actions]"));

    // Round-trip
    let loaded = parse(file.path()).unwrap();
    assert!(loaded.value.has(&make_key("actions/checkout", "^4")));
    assert!(loaded.value.has(&make_key("actions/setup-node", "^3")));
    assert!(loaded.value.has(&make_key("actions/cache", "^3")));
    let entry = loaded
        .value
        .get(&make_key("actions/checkout", "^4"))
        .unwrap();
    assert_eq!(
        entry.sha,
        CommitSha::from("abc123def456789012345678901234567890abcd")
    );
    assert_eq!(entry.version, Some("v4.1.0".to_owned()));
    assert_eq!(entry.comment, "v4");
}

#[test]
fn test_create_roundtrip_matches_domain_state() {
    let file = NamedTempFile::new().unwrap();

    let key = make_key("actions/checkout", "^4");
    let entry = LockEntry::with_version_and_comment(
        CommitSha::from("abc123def456789012345678901234567890abcd"),
        Some("v4.2.0".to_owned()),
        "v4".to_owned(),
        "actions/checkout".to_owned(),
        Some(RefType::Release),
        "2026-01-15T10:30:00Z".to_owned(),
    );

    let diff = LockDiff {
        added: vec![(key.clone(), entry.clone())],
        ..Default::default()
    };
    create(file.path(), &diff).unwrap();

    let loaded = parse(file.path()).unwrap();
    let loaded_entry = loaded.value.get(&key).expect("Entry must exist");
    assert_eq!(loaded_entry.sha, entry.sha);
    assert_eq!(loaded_entry.version, entry.version);
    assert_eq!(loaded_entry.comment, entry.comment);
    assert_eq!(loaded_entry.repository, entry.repository);
    assert_eq!(loaded_entry.ref_type, entry.ref_type);
    assert_eq!(loaded_entry.date, entry.date);
}

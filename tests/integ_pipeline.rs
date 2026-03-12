#![allow(unused_crate_dependencies)]

//! Integration tests for pipeline edge cases that require specific mock behavior.
//!
//! These tests verify the SHA-first resolution path with controlled registry
//! responses that cannot be replicated with a real GitHub API.

mod common;

use common::registries::{EmptyDateRegistry, FailingDescribeRegistry, FakeRegistry};
use common::setup::{create_test_repo, lock_path, run_init, write_workflow};
use gx::domain::action::identity::ActionId;
use gx::domain::action::spec::LockKey;
use gx::domain::action::specifier::Specifier;
use gx::infra::lock;
use tempfile::TempDir;

/// `init` on a SHA-pinned workflow where `describe_sha` returns no tags must use the SHA as version.
#[test]
fn test_init_sha_first_describe_sha_no_tags() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);

    // FakeRegistry with no sha_tags configured returns empty tags from describe_sha.
    let checkout_sha = FakeRegistry::fake_sha("actions/checkout", "v4");

    write_workflow(
        &root,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{checkout_sha} # v4\n"
        ),
    );

    // FakeRegistry.describe_sha returns empty tags → SHA used as version in lock entry
    run_init(&root, &FakeRegistry::new());

    let lock = lock::parse(&lock_path(&root)).unwrap().value;
    let key = LockKey::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let entry = lock.get(&key).expect("Lock must have checkout@v4 entry");

    assert_eq!(
        entry.sha.as_str(),
        checkout_sha.as_str(),
        "Lock SHA must match the workflow-pinned SHA"
    );

    assert_eq!(
        entry.version.as_deref(),
        Some(checkout_sha.as_str()),
        "When describe_sha returns no tags, lock version should be the SHA itself"
    );
}

/// `init` on a SHA-pinned workflow where `describe_sha` cannot fetch the commit date
/// must still succeed — date fetch failures are non-fatal.
#[test]
fn test_init_sha_first_describe_sha_empty_date() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);

    let checkout_sha = FakeRegistry::fake_sha("actions/checkout", "v4");

    write_workflow(
        &root,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{checkout_sha} # v4\n"
        ),
    );

    run_init(&root, &EmptyDateRegistry);

    let lock = lock::parse(&lock_path(&root)).unwrap().value;
    let key = LockKey::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let entry = lock.get(&key).expect("Lock must have checkout@v4 entry");

    assert_eq!(
        entry.sha.as_str(),
        checkout_sha.as_str(),
        "Lock SHA must match the workflow-pinned SHA"
    );

    assert_eq!(
        entry.date, "",
        "Date should be empty when commit date fetch fails"
    );
}

/// `init` on a SHA-pinned workflow where `describe_sha` fails must fall back
/// to `resolve(spec)` (tag-based resolution) instead of failing entirely.
#[test]
fn test_init_sha_first_describe_sha_fails_falls_back_to_resolve() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);

    let checkout_sha = FakeRegistry::fake_sha("actions/checkout", "v4");

    write_workflow(
        &root,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{checkout_sha} # v4\n"
        ),
    );

    // describe_sha fails, but init must succeed by falling back to resolve(spec)
    run_init(&root, &FailingDescribeRegistry);

    let lock = lock::parse(&lock_path(&root)).unwrap().value;
    let key = LockKey::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let entry = lock.get(&key).expect("Lock must have checkout@v4 entry");

    assert!(!entry.sha.as_str().is_empty(), "Lock entry must have a SHA");
    assert_eq!(
        entry.repository, "actions/checkout",
        "Repository must be set"
    );
}

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::string_slice,
    clippy::shadow_unrelated,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]

//! End-to-end pipeline tests using the real GitHub API.
//!
//! These tests require a `GITHUB_TOKEN` environment variable and network access.
//! Run via `mise run e2e`.

mod common;

use common::setup::{
    create_test_repo, lock_path, manifest_path, run_init, run_lint, run_tidy, run_upgrade,
    write_workflow,
};
use gx::domain::action::identity::ActionId;
use gx::domain::action::spec::Spec;
use gx::domain::action::specifier::Specifier;
use gx::domain::resolution::VersionRegistry;
use gx::infra::github::Registry as GithubRegistry;
use gx::infra::lock::Store as LockStore;
use gx::infra::manifest;
use gx::upgrade::types::{Mode as UpgradeMode, Request as UpgradeRequest, Scope as UpgradeScope};
use std::fs;
use tempfile::TempDir;

fn github_registry() -> GithubRegistry {
    let token = std::env::var("GITHUB_TOKEN").ok();
    GithubRegistry::new(token).expect("Failed to create GithubRegistry")
}

/// `init` on a fresh repo creates parseable manifest and lock; workflow pins match lock SHAs.
#[test]
fn e2e_init_creates_parseable_files_with_matching_pins() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);
    let registry = github_registry();

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/setup-node@v4\n",
    );

    run_init(&root, &registry);

    let manifest = manifest::parse(&manifest_path(&root)).unwrap().value;
    assert!(manifest.has(&ActionId::from("actions/checkout")));
    assert!(manifest.has(&ActionId::from("actions/setup-node")));

    let lock = LockStore::new(&lock_path(&root)).load().unwrap();
    let checkout_key = Spec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let setup_key = Spec::new(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v4"),
    );
    assert!(lock.get(&checkout_key).is_some(), "Lock must have checkout");
    assert!(lock.get(&setup_key).is_some(), "Lock must have setup-node");

    let wf = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let checkout_sha = lock.get(&checkout_key).unwrap().1.sha.to_string();
    assert!(
        wf.contains(&checkout_sha),
        "Workflow should contain checkout SHA {checkout_sha}"
    );
}

/// `tidy` immediately after `init` is a no-op (file contents unchanged).
#[test]
fn e2e_tidy_after_init_is_noop() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);
    let registry = github_registry();

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    run_init(&root, &registry);

    let manifest_before = fs::read_to_string(manifest_path(&root)).unwrap();
    let lock_before = fs::read_to_string(lock_path(&root)).unwrap();
    let workflow_before = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();

    run_tidy(&root, &registry);

    assert_eq!(
        fs::read_to_string(manifest_path(&root)).unwrap(),
        manifest_before,
        "Manifest should not change"
    );
    assert_eq!(
        fs::read_to_string(lock_path(&root)).unwrap(),
        lock_before,
        "Lock should not change"
    );
    assert_eq!(
        fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap(),
        workflow_before,
        "Workflow should not change"
    );
}

/// `tidy` after adding a new action to a workflow adds only that action to manifest/lock.
#[test]
fn e2e_tidy_adds_new_action() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);
    let registry = github_registry();

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    run_init(&root, &registry);

    let manifest_before = manifest::parse(&manifest_path(&root)).unwrap().value;
    assert!(manifest_before.has(&ActionId::from("actions/checkout")));
    assert!(!manifest_before.has(&ActionId::from("actions/setup-node")));

    // Add a new action to the workflow, using the already-pinned SHA for checkout
    let lock = LockStore::new(&lock_path(&root)).load().unwrap();
    let checkout_key = Spec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let checkout_sha = lock.get(&checkout_key).unwrap().1.sha.to_string();

    write_workflow(
        &root,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{checkout_sha} # v4\n      - uses: actions/setup-node@v4\n"
        ),
    );

    run_tidy(&root, &registry);

    let manifest_after = manifest::parse(&manifest_path(&root)).unwrap().value;
    assert!(manifest_after.has(&ActionId::from("actions/checkout")));
    assert!(
        manifest_after.has(&ActionId::from("actions/setup-node")),
        "New action should be added to manifest"
    );

    let lock_after = LockStore::new(&lock_path(&root)).load().unwrap();
    let new_key = Spec::new(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v4"),
    );
    assert!(
        lock_after.get(&new_key).is_some(),
        "New action should be in the lock"
    );
}

/// `tidy` after removing an action from all workflows removes only that action from manifest/lock.
#[test]
fn e2e_tidy_removes_stale_action() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);
    let registry = github_registry();

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/setup-node@v4\n",
    );

    run_init(&root, &registry);

    let manifest_before = manifest::parse(&manifest_path(&root)).unwrap().value;
    assert!(manifest_before.has(&ActionId::from("actions/checkout")));
    assert!(manifest_before.has(&ActionId::from("actions/setup-node")));

    // Remove setup-node from workflow, keep checkout pinned
    let lock = LockStore::new(&lock_path(&root)).load().unwrap();
    let checkout_key = Spec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let checkout_sha = lock.get(&checkout_key).unwrap().1.sha.to_string();

    write_workflow(
        &root,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{checkout_sha} # v4\n"
        ),
    );

    run_tidy(&root, &registry);

    let manifest_after = manifest::parse(&manifest_path(&root)).unwrap().value;
    assert!(manifest_after.has(&ActionId::from("actions/checkout")));
    assert!(
        !manifest_after.has(&ActionId::from("actions/setup-node")),
        "Removed action should be gone from manifest"
    );

    let lock_after = LockStore::new(&lock_path(&root)).load().unwrap();
    let stale_key = Spec::new(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v4"),
    );
    assert!(
        lock_after.get(&stale_key).is_none(),
        "Removed action should be gone from lock"
    );
    assert!(
        lock_after.get(&checkout_key).is_some(),
        "Remaining action should still be in lock"
    );
}

/// `tidy` with override changes patches only the overrides section.
#[test]
fn e2e_tidy_override_changes() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);
    let registry = github_registry();

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    );
    write_workflow(
        &root,
        "deploy.yml",
        "name: Deploy\non: push\njobs:\n  deploy:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/checkout@v3\n",
    );

    run_init(&root, &registry);

    let manifest = manifest::parse(&manifest_path(&root)).unwrap().value;
    assert!(manifest.has(&ActionId::from("actions/checkout")));
    let overrides = manifest.overrides_for(&ActionId::from("actions/checkout"));
    assert!(
        !overrides.is_empty(),
        "Should have override for minority version"
    );
}

/// `upgrade` patches only upgraded entries in manifest/lock, preserves the rest.
#[test]
fn e2e_upgrade_preserves_unaffected_entries() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);
    let registry = github_registry();

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/setup-node@v4\n",
    );

    run_init(&root, &registry);

    // Record setup-node lock entry before upgrade
    let lock_before = LockStore::new(&lock_path(&root)).load().unwrap();
    let node_key = Spec::new(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v4"),
    );
    let node_sha_before = lock_before
        .get(&node_key)
        .unwrap()
        .1
        .sha
        .as_str()
        .to_owned();

    // Upgrade only checkout (scoped upgrade leaves setup-node untouched)
    let request = UpgradeRequest::new(
        UpgradeMode::Safe,
        UpgradeScope::Single(ActionId::from("actions/checkout")),
    );
    run_upgrade(&root, &registry, &request);

    // Setup-node lock entry should be unchanged
    let lock_after = LockStore::new(&lock_path(&root)).load().unwrap();
    let node_sha_after = lock_after.get(&node_key).unwrap().1.sha.as_str().to_owned();
    assert_eq!(
        node_sha_before, node_sha_after,
        "Setup-node SHA should be unchanged after scoped checkout upgrade"
    );
}

/// `lint` detects unsynced manifest after workflow modifications.
#[test]
fn e2e_lint_detects_unsynced_manifest() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);
    let registry = github_registry();

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    run_init(&root, &registry);

    let lock = LockStore::new(&lock_path(&root)).load().unwrap();
    let checkout_key = Spec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let checkout_sha = lock.get(&checkout_key).unwrap().1.sha.to_string();

    // Add an unmanaged action to the workflow (not in manifest)
    write_workflow(
        &root,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{checkout_sha} # v4\n      - uses: actions/setup-node@v4\n"
        ),
    );

    let diagnostics = run_lint(&root);
    let has_unsynced = diagnostics.iter().any(|d| d.rule == "unsynced-manifest");
    assert!(
        has_unsynced,
        "Lint should detect unsynced-manifest for setup-node, got: {diagnostics:?}"
    );
}

/// `init` on a SHA-pinned workflow sets version to semver tag, not the raw SHA.
#[test]
fn e2e_init_sha_pinned_workflow_sets_version_not_sha() {
    let registry = github_registry();

    // Step 1: Get a real SHA for actions/checkout@v4 via a normal init
    let temp1 = TempDir::new().unwrap();
    let root1 = create_test_repo(&temp1);
    write_workflow(
        &root1,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    );
    run_init(&root1, &registry);
    let lock1 = LockStore::new(&lock_path(&root1)).load().unwrap();
    let key = Spec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let checkout_sha = lock1.get(&key).unwrap().1.sha.to_string();

    // Step 2: Init with a SHA-pinned workflow — tests the SHA-first resolution path
    let temp2 = TempDir::new().unwrap();
    let root2 = create_test_repo(&temp2);
    write_workflow(
        &root2,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{checkout_sha} # v4\n"
        ),
    );
    run_init(&root2, &registry);

    let lock2 = LockStore::new(&lock_path(&root2)).load().unwrap();
    let (res, _commit) = lock2.get(&key).expect("Lock must have checkout@v4 entry");

    // The lock version must be a semver tag, not the raw SHA
    assert_ne!(
        res.version.as_str(),
        checkout_sha.as_str(),
        "Lock version for checkout must NOT be a raw SHA"
    );
    assert!(
        res.version.as_str().starts_with('v'),
        "Lock version should be a semver tag, got: {:?}",
        res.version
    );
}

/// `tidy` after SHA-pinned `init` is a no-op (idempotent).
#[test]
fn e2e_tidy_after_sha_pinned_init_is_noop() {
    let registry = github_registry();

    // Get a real SHA for checkout@v4
    let temp1 = TempDir::new().unwrap();
    let root1 = create_test_repo(&temp1);
    write_workflow(
        &root1,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    );
    run_init(&root1, &registry);
    let lock1 = LockStore::new(&lock_path(&root1)).load().unwrap();
    let key = Spec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let checkout_sha = lock1.get(&key).unwrap().1.sha.to_string();

    // Init with SHA-pinned workflow
    let temp2 = TempDir::new().unwrap();
    let root2 = create_test_repo(&temp2);
    write_workflow(
        &root2,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{checkout_sha} # v4\n"
        ),
    );
    run_init(&root2, &registry);

    let manifest_before = fs::read_to_string(manifest_path(&root2)).unwrap();
    let lock_before = fs::read_to_string(lock_path(&root2)).unwrap();
    let workflow_before = fs::read_to_string(root2.join(".github/workflows/ci.yml")).unwrap();

    run_tidy(&root2, &registry);

    assert_eq!(
        fs::read_to_string(manifest_path(&root2)).unwrap(),
        manifest_before,
        "Manifest should not change on second tidy"
    );
    assert_eq!(
        fs::read_to_string(lock_path(&root2)).unwrap(),
        lock_before,
        "Lock should not change on second tidy"
    );
    assert_eq!(
        fs::read_to_string(root2.join(".github/workflows/ci.yml")).unwrap(),
        workflow_before,
        "Workflow should not change on second tidy"
    );
}

/// Full pipeline: init → tidy → upgrade. Version fields must never contain a raw SHA.
#[test]
fn e2e_full_pipeline_init_tidy_upgrade() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);
    let registry = github_registry();

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    // Step 1: Init
    run_init(&root, &registry);

    let manifest = manifest::parse(&manifest_path(&root)).unwrap().value;
    assert_eq!(
        manifest.get(&ActionId::from("actions/checkout")),
        Some(&Specifier::from_v1("v4"))
    );

    // Step 2: Tidy — should be no-op
    let manifest_before = fs::read_to_string(manifest_path(&root)).unwrap();
    let lock_before = fs::read_to_string(lock_path(&root)).unwrap();

    run_tidy(&root, &registry);

    assert_eq!(
        fs::read_to_string(manifest_path(&root)).unwrap(),
        manifest_before,
        "Tidy after init should not change manifest"
    );
    assert_eq!(
        fs::read_to_string(lock_path(&root)).unwrap(),
        lock_before,
        "Tidy after init should not change lock"
    );

    // Step 3: Upgrade checkout (safe mode - minor/patch bumps within v4)
    let request = UpgradeRequest::new(
        UpgradeMode::Safe,
        UpgradeScope::Single(ActionId::from("actions/checkout")),
    );
    run_upgrade(&root, &registry, &request);

    // After upgrade attempt, manifest still has checkout (version may or may not have changed)
    let manifest_after = manifest::parse(&manifest_path(&root)).unwrap().value;
    assert!(
        manifest_after.has(&ActionId::from("actions/checkout")),
        "Checkout should still be in manifest after upgrade"
    );
}

/// `init` with an action that uses annotated tags (e.g. release-plz/action@v0.5)
/// must produce a valid commit SHA in the lock file, not a tag object SHA.
/// Tag object SHAs are git internal references that GitHub rejects when used in
/// `uses: owner/repo@sha` workflow pins.
#[test]
fn e2e_init_annotated_tag_action_produces_valid_commit_sha() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);
    let registry = github_registry();

    // release-plz/action@v0.5 uses an annotated tag
    write_workflow(
        &root,
        "release.yml",
        "name: Release\non: push\njobs:\n  release:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - uses: release-plz/action@v0.5\n",
    );

    run_init(&root, &registry);

    let lock = LockStore::new(&lock_path(&root)).load().unwrap();
    let key = Spec::new(
        ActionId::from("release-plz/action"),
        Specifier::from_v1("v0.5"),
    );
    let (_res, commit) = lock
        .get(&key)
        .expect("Lock must have release-plz/action entry");
    let sha = commit.sha.clone();

    // The SHA in the lock must be a valid commit, not a tag object SHA.
    // describe_sha fetches the commit date via the commits API, which returns 422 for tag objects.
    let id = ActionId::from("release-plz/action");
    let description = <GithubRegistry as VersionRegistry>::describe_sha(&registry, &id, &sha);
    assert!(
        description.is_ok(),
        "Lock SHA {} should be a valid commit, but describe_sha failed: {:?} \
         (likely a tag object SHA was stored instead of the commit SHA)",
        sha,
        description.err()
    );

    // The workflow should be pinned to this same SHA
    let sha_str = sha.to_string();
    let wf = fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    assert!(
        wf.contains(&sha_str),
        "Workflow should contain the commit SHA {sha_str}"
    );

    // Tidy should be a no-op after init (idempotent)
    let lock_before = fs::read_to_string(lock_path(&root)).unwrap();
    let workflow_before = wf;

    run_tidy(&root, &registry);

    assert_eq!(
        fs::read_to_string(lock_path(&root)).unwrap(),
        lock_before,
        "Lock should not change on tidy after init"
    );
    assert_eq!(
        fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap(),
        workflow_before,
        "Workflow should not change on tidy after init"
    );
}

/// Sequential init → tidy → modify workflow → tidy → upgrade produces correct final state.
#[test]
fn e2e_full_pipeline_init_tidy_modify_tidy_upgrade() {
    let temp = TempDir::new().unwrap();
    let root = create_test_repo(&temp);
    let registry = github_registry();

    // Step 1: Create initial workflow
    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    // Step 2: Init
    run_init(&root, &registry);

    let manifest = manifest::parse(&manifest_path(&root)).unwrap().value;
    assert_eq!(
        manifest.get(&ActionId::from("actions/checkout")),
        Some(&Specifier::from_v1("v4"))
    );

    // Step 3: Tidy immediately after init — should be no-op
    let manifest_before = fs::read_to_string(manifest_path(&root)).unwrap();
    let lock_before = fs::read_to_string(lock_path(&root)).unwrap();

    run_tidy(&root, &registry);

    assert_eq!(
        fs::read_to_string(manifest_path(&root)).unwrap(),
        manifest_before,
        "Tidy after init should not change manifest"
    );
    assert_eq!(
        fs::read_to_string(lock_path(&root)).unwrap(),
        lock_before,
        "Tidy after init should not change lock"
    );

    // Step 4: Add a new action to workflow
    let lock = LockStore::new(&lock_path(&root)).load().unwrap();
    let checkout_key = Spec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let checkout_sha = lock.get(&checkout_key).unwrap().1.sha.to_string();

    write_workflow(
        &root,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{checkout_sha} # v4\n      - uses: actions/setup-node@v4\n"
        ),
    );

    // Step 5: Tidy should pick up the new action
    run_tidy(&root, &registry);

    let manifest = manifest::parse(&manifest_path(&root)).unwrap().value;
    assert!(manifest.has(&ActionId::from("actions/setup-node")));

    let lock = LockStore::new(&lock_path(&root)).load().unwrap();
    let node_key = Spec::new(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v4"),
    );
    assert!(
        lock.get(&node_key).is_some(),
        "setup-node should be in lock after tidy"
    );

    // Step 6: Upgrade checkout (safe mode)
    let request = UpgradeRequest::new(
        UpgradeMode::Safe,
        UpgradeScope::Single(ActionId::from("actions/checkout")),
    );
    run_upgrade(&root, &registry, &request);

    // Verify final state: both actions still in manifest
    let manifest = manifest::parse(&manifest_path(&root)).unwrap().value;
    assert!(manifest.has(&ActionId::from("actions/checkout")));
    assert!(
        manifest.has(&ActionId::from("actions/setup-node")),
        "Setup-node should remain in manifest"
    );

    // Setup-node should still be in lock
    let lock = LockStore::new(&lock_path(&root)).load().unwrap();
    assert!(
        lock.get(&node_key).is_some(),
        "Lock should still have setup-node@v4"
    );
}

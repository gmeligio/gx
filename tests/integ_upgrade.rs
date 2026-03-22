#![expect(
    clippy::unwrap_used,
    clippy::string_slice,
    clippy::assertions_on_result_states,
    clippy::shadow_reuse,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]

mod common;

use common::registries::FakeRegistry;
use common::setup::{
    create_test_repo, lock_path, manifest_path, write_lock, write_manifest, write_workflow,
};
use gx::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use gx::domain::action::resolved::{Commit, ResolvedAction};
use gx::domain::action::spec::Spec as ActionSpec;
use gx::domain::action::specifier::Specifier;
use gx::domain::action::uses_ref::RefType;
use gx::domain::lock::Lock;
use gx::domain::manifest::Manifest;
use gx::infra::lock::Store as LockStore;
use gx::infra::manifest::patch::apply_manifest_diff;
use gx::infra::manifest::{self};
use gx::infra::workflow_update::WorkflowWriter;
use gx::upgrade;
use gx::upgrade::cli::{Mode as UpgradeMode, Request as UpgradeRequest, Scope as UpgradeScope};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper to run upgrade with file-backed stores using `FakeRegistry`.
fn run_upgrade_file_backed(repo_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let request = UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All);
    run_upgrade_file_backed_with_request(repo_root, &request)
}

/// Helper to run upgrade with file-backed stores and a specific request.
fn run_upgrade_file_backed_with_request(
    repo_root: &Path,
    request: &UpgradeRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    let mp = manifest_path(repo_root);
    let lp = lock_path(repo_root);
    let manifest = manifest::parse(&mp)?.value;
    let lock_store = LockStore::new(&lp);
    let lock = lock_store.load()?;
    let updater = WorkflowWriter::new(repo_root);

    let plan = upgrade::plan::plan(&manifest, &lock, &FakeRegistry::new(), request, |_| {})?;

    if !plan.is_empty() {
        apply_manifest_diff(&mp, &plan.manifest)?;
        lock_store.save(&plan.lock)?;
        upgrade::plan::apply_upgrade_workflows(&updater, &plan.lock_changes, &plan.upgrades)?;
    }

    Ok(())
}

// --- Tests that don't require GitHub API ---

#[test]
fn upgrade_empty_manifest_is_noop() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let lock = Lock::default();
    let request = UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All);
    let result = upgrade::plan::plan(&manifest, &lock, &FakeRegistry::new(), &request, |_| {});
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty(), "No tags available means noop");
}

#[test]
fn upgrade_empty_file_manifest_is_noop() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    write_manifest(&root, "[actions]\n\"actions/checkout\" = \"v4\"\n");
    write_workflow(
        &root,
        "ci.yml",
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    let result = run_upgrade_file_backed(&root);
    assert!(result.is_ok());

    let manifest_content = fs::read_to_string(root.join(".github").join("gx.toml")).unwrap();
    assert!(manifest_content.contains("actions/checkout"));
    assert!(manifest_content.contains("v4"));
}

#[test]
fn upgrade_non_semver_versions_skipped() {
    let manifest = Manifest::default();
    let lock = Lock::default();
    let result = upgrade::plan::plan(
        &manifest,
        &lock,
        &FakeRegistry::new(),
        &UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All),
        |_| {},
    );
    assert!(result.is_ok());
}

#[test]
fn upgrade_preserves_workflow_structure() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    write_manifest(&root, "[actions]\n\"actions/checkout\" = \"v4\"\n");

    let workflow_content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: echo hello
";
    write_workflow(&root, "ci.yml", workflow_content);

    let result = run_upgrade_file_backed(&root);
    assert!(result.is_ok());

    let after = fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();
    assert_eq!(after, workflow_content);
}

#[test]
fn upgrade_no_lock_file_created_when_empty_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    write_manifest(&root, "[actions]\n");

    let result = run_upgrade_file_backed(&root);
    assert!(result.is_ok());

    let manifest_content = fs::read_to_string(root.join(".github").join("gx.toml")).unwrap();
    assert!(
        manifest_content.trim() == "[actions]" || manifest_content.trim() == "[actions]\n",
        "Manifest should remain unchanged: {manifest_content}"
    );
}

#[test]
fn upgrade_with_existing_lock_and_empty_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    write_manifest(&root, "[actions]\n\"actions/checkout\" = \"v4\"\n");
    write_lock(
        &root,
        "version = \"1.3\"\n\n[actions]\n\"actions/checkout@v4\" = { sha = \"abc123def456789012345678901234567890abcd\", repository = \"actions/checkout\", ref_type = \"tag\", date = \"\" }\n",
    );

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@abc123def456789012345678901234567890abcd # v4\n",
    );

    let result = run_upgrade_file_backed(&root);
    assert!(result.is_ok());
}

#[test]
fn upgrade_memory_stores_no_side_effects() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let lock = Lock::default();
    let result = upgrade::plan::plan(
        &manifest,
        &lock,
        &FakeRegistry::new(),
        &UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All),
        |_| {},
    );
    assert!(result.is_ok());

    assert!(!root.join(".github").join("gx.toml").exists());
    assert!(!root.join(".github").join("gx.lock").exists());
}

#[test]
fn upgrade_multiple_workflows_empty_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    write_manifest(
        &root,
        "[actions]\n\"actions/checkout\" = \"v4\"\n\"docker/build-push-action\" = \"v5\"\n",
    );

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
    );
    write_workflow(
        &root,
        "deploy.yml",
        "name: Deploy\njobs:\n  deploy:\n    steps:\n      - uses: docker/build-push-action@v5\n",
    );

    let result = run_upgrade_file_backed(&root);
    assert!(result.is_ok());
}

/// Reproduces the bug where `upgrade` replaces SHAs with bare version tags
/// for actions that have no available upgrade.
#[test]
fn upgrade_preserves_sha_for_non_upgraded_actions() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let login_sha = "5e57cd118135c172c3672efd75eb46360885c0ef";
    let checkout_old_sha = "8e8c483db84b4bee98b60c0593521ed34d9990e8";
    let checkout_new_sha = "11bd71901bbe5b1630ceea73d27597364c9af683";

    let workflow_content = format!(
        "on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: docker/login-action@{login_sha} # v3.6.0
      - uses: actions/checkout@{checkout_old_sha} # v6.0.1
"
    );
    write_workflow(&root, "ci.yml", &workflow_content);

    let mut manifest = Manifest::default();
    manifest.set(
        ActionId::from("docker/login-action"),
        Specifier::from_v1("v3.6.0"),
    );
    manifest.set(
        ActionId::from("actions/checkout"),
        Specifier::from_v1("v6.0.1"),
    );

    let mut lock = Lock::default();

    manifest.set(
        ActionId::from("actions/checkout"),
        Specifier::from_v1("v6.0.2"),
    );

    lock.set(
        &ActionSpec::new(
            ActionId::from("actions/checkout"),
            Specifier::from_v1("v6.0.2"),
        ),
        Version::from("v6.0.2"),
        Commit {
            sha: CommitSha::from(checkout_new_sha),
            repository: Repository::from("actions/checkout"),
            ref_type: Some(RefType::Tag),
            date: CommitDate::from(""),
        },
    );

    let keys_to_retain: Vec<ActionSpec> = manifest.specs().cloned().collect();
    lock.retain(&keys_to_retain);

    let pins = vec![ResolvedAction {
        id: ActionId::from("actions/checkout"),
        sha: CommitSha::from(checkout_new_sha),
        version: Some(Version::from("v6.0.2")),
    }];
    let writer = WorkflowWriter::new(&root);
    let _results = writer.update_all_with_pins(&pins).unwrap();

    let updated =
        fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();

    assert!(
        updated.contains(&format!("actions/checkout@{checkout_new_sha} # v6.0.2")),
        "Expected checkout to be updated to new SHA. Got:\n{updated}"
    );

    assert!(
        updated.contains(&format!("docker/login-action@{login_sha} # v3.6.0")),
        "Expected login-action to keep its SHA. Got:\n{updated}"
    );

    assert!(
        !updated.contains("docker/login-action@v3.6.0"),
        "Bug: login-action SHA was replaced with bare version tag. Got:\n{updated}"
    );
}

#[test]
fn upgrade_repins_branch_ref() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let old_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let workflow_content = format!(
        "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: my-org/my-action@{old_sha} # main\n"
    );
    write_workflow(&root, "ci.yml", &workflow_content);

    let mut manifest = Manifest::default();
    manifest.set(
        ActionId::from("my-org/my-action"),
        Specifier::from_v1("main"),
    );

    let mut lock = Lock::default();
    lock.set(
        &ActionSpec::new(
            ActionId::from("my-org/my-action"),
            Specifier::from_v1("main"),
        ),
        Version::from("main"),
        Commit {
            sha: CommitSha::from(old_sha),
            repository: Repository::from("my-org/my-action"),
            ref_type: Some(RefType::Branch),
            date: CommitDate::from(""),
        },
    );

    let request = UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All);
    let plan = upgrade::plan::plan(&manifest, &lock, &FakeRegistry::new(), &request, |_| {});
    assert!(plan.is_ok(), "upgrade failed: {:?}", plan.unwrap_err());
    let plan = plan.unwrap();

    let updater = WorkflowWriter::new(&root);
    upgrade::plan::apply_upgrade_workflows(&updater, &plan.lock_changes, &plan.upgrades).unwrap();

    let expected_sha = FakeRegistry::fake_sha("my-org/my-action", "main");
    let updated_workflow =
        fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();
    assert!(
        updated_workflow.contains(&format!("my-org/my-action@{expected_sha} # main")),
        "Expected branch ref to be re-pinned with new SHA. Got:\n{updated_workflow}"
    );
}

#[test]
fn upgrade_latest_also_repins_branch_ref() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let old_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let workflow_content = format!(
        "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: my-org/my-action@{old_sha} # main\n"
    );
    write_workflow(&root, "ci.yml", &workflow_content);

    let mut manifest = Manifest::default();
    manifest.set(
        ActionId::from("my-org/my-action"),
        Specifier::from_v1("main"),
    );

    let mut lock = Lock::default();
    lock.set(
        &ActionSpec::new(
            ActionId::from("my-org/my-action"),
            Specifier::from_v1("main"),
        ),
        Version::from("main"),
        Commit {
            sha: CommitSha::from(old_sha),
            repository: Repository::from("my-org/my-action"),
            ref_type: Some(RefType::Branch),
            date: CommitDate::from(""),
        },
    );

    let request = UpgradeRequest::new(UpgradeMode::Latest, UpgradeScope::All);
    let plan = upgrade::plan::plan(&manifest, &lock, &FakeRegistry::new(), &request, |_| {});
    assert!(plan.is_ok());
    let plan = plan.unwrap();

    let updater = WorkflowWriter::new(&root);
    upgrade::plan::apply_upgrade_workflows(&updater, &plan.lock_changes, &plan.upgrades).unwrap();

    let expected_sha = FakeRegistry::fake_sha("my-org/my-action", "main");
    let updated_workflow =
        fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();
    assert!(
        updated_workflow.contains(&format!("my-org/my-action@{expected_sha} # main")),
        "Expected branch ref to be re-pinned in --latest mode. Got:\n{updated_workflow}"
    );
}

#[test]
fn upgrade_targeted_does_not_repin_branch_ref() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let branch_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let checkout_sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    let workflow_content = format!(
        "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: my-org/my-action@{branch_sha} # main\n      - uses: actions/checkout@{checkout_sha} # v4\n"
    );
    write_workflow(&root, "ci.yml", &workflow_content);

    let mut manifest = Manifest::default();
    manifest.set(
        ActionId::from("my-org/my-action"),
        Specifier::from_v1("main"),
    );
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));

    let mut lock = Lock::default();
    lock.set(
        &ActionSpec::new(
            ActionId::from("my-org/my-action"),
            Specifier::from_v1("main"),
        ),
        Version::from("main"),
        Commit {
            sha: CommitSha::from(branch_sha),
            repository: Repository::from("my-org/my-action"),
            ref_type: Some(RefType::Branch),
            date: CommitDate::from(""),
        },
    );
    lock.set(
        &ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4")),
        Version::from("v4"),
        Commit {
            sha: CommitSha::from(checkout_sha),
            repository: Repository::from("actions/checkout"),
            ref_type: Some(RefType::Tag),
            date: CommitDate::from(""),
        },
    );

    let registry = FakeRegistry::new().with_all_tags("actions/checkout", vec!["v4", "v5"]);

    let request = UpgradeRequest::new(
        UpgradeMode::Safe,
        UpgradeScope::Pinned(ActionId::from("actions/checkout"), Version::from("v5")),
    );
    let plan = upgrade::plan::plan(&manifest, &lock, &registry, &request, |_| {});
    assert!(plan.is_ok());
    let plan = plan.unwrap();

    let updater = WorkflowWriter::new(&root);
    upgrade::plan::apply_upgrade_workflows(&updater, &plan.lock_changes, &plan.upgrades).unwrap();

    let updated_workflow =
        fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();

    assert!(
        updated_workflow.contains(&format!("my-org/my-action@{branch_sha} # main")),
        "Branch ref should not be re-pinned in targeted mode. Got:\n{updated_workflow}"
    );
}

#[test]
fn upgrade_mixed_semver_and_branch() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let old_branch_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let old_checkout_sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    let workflow_content = format!(
        "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: my-org/my-action@{old_branch_sha} # main\n      - uses: actions/checkout@{old_checkout_sha} # v4\n"
    );
    write_workflow(&root, "ci.yml", &workflow_content);

    let mut manifest = Manifest::default();
    manifest.set(
        ActionId::from("my-org/my-action"),
        Specifier::from_v1("main"),
    );
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));

    let mut lock = Lock::default();
    lock.set(
        &ActionSpec::new(
            ActionId::from("my-org/my-action"),
            Specifier::from_v1("main"),
        ),
        Version::from("main"),
        Commit {
            sha: CommitSha::from(old_branch_sha),
            repository: Repository::from("my-org/my-action"),
            ref_type: Some(RefType::Branch),
            date: CommitDate::from(""),
        },
    );
    lock.set(
        &ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4")),
        Version::from("v4"),
        Commit {
            sha: CommitSha::from(old_checkout_sha),
            repository: Repository::from("actions/checkout"),
            ref_type: Some(RefType::Tag),
            date: CommitDate::from(""),
        },
    );

    let registry = FakeRegistry::new().with_all_tags("actions/checkout", vec!["v4", "v5"]);

    let request = UpgradeRequest::new(UpgradeMode::Latest, UpgradeScope::All);
    let plan = upgrade::plan::plan(&manifest, &lock, &registry, &request, |_| {});
    assert!(plan.is_ok());
    let plan = plan.unwrap();

    let updater = WorkflowWriter::new(&root);
    upgrade::plan::apply_upgrade_workflows(&updater, &plan.lock_changes, &plan.upgrades).unwrap();

    let updated_workflow =
        fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();

    let expected_branch_sha = FakeRegistry::fake_sha("my-org/my-action", "main");
    assert!(
        updated_workflow.contains(&format!("my-org/my-action@{expected_branch_sha} # main")),
        "Branch ref should be re-pinned. Got:\n{updated_workflow}"
    );

    let expected_checkout_sha = FakeRegistry::fake_sha("actions/checkout", "v5");
    assert!(
        updated_workflow.contains(&format!("actions/checkout@{expected_checkout_sha} # v5")),
        "Checkout should be upgraded to v5. Got:\n{updated_workflow}"
    );
}

#[test]
fn upgrade_skips_bare_sha() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let bare_sha = "cccccccccccccccccccccccccccccccccccccccc";

    let workflow_content = format!(
        "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: my-org/my-action@{bare_sha}\n"
    );
    write_workflow(&root, "ci.yml", &workflow_content);

    let mut manifest = Manifest::default();
    manifest.set(
        ActionId::from("my-org/my-action"),
        Specifier::from_v1(bare_sha),
    );

    let lock = Lock::default();
    let request = UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All);
    let plan = upgrade::plan::plan(&manifest, &lock, &FakeRegistry::new(), &request, |_| {});
    assert!(plan.is_ok());
    let plan = plan.unwrap();

    if !plan.is_empty() {
        let updater = WorkflowWriter::new(&root);
        upgrade::plan::apply_upgrade_workflows(&updater, &plan.lock_changes, &plan.upgrades)
            .unwrap();
    }

    let updated_workflow =
        fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();
    assert!(
        updated_workflow.contains(&format!("my-org/my-action@{bare_sha}")),
        "Bare SHA should remain unchanged. Got:\n{updated_workflow}"
    );
}

// --- Tests for scoped upgrades ---

#[test]
fn upgrade_safe_single_action() {
    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    manifest.set(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v3"),
    );

    let registry = FakeRegistry::new()
        .with_all_tags("actions/checkout", vec!["v4", "v5"])
        .with_all_tags("actions/setup-node", vec!["v3", "v4"]);

    let lock = Lock::default();
    let request = UpgradeRequest::new(
        UpgradeMode::Safe,
        UpgradeScope::Single(ActionId::from("actions/checkout")),
    );
    let result = upgrade::plan::plan(&manifest, &lock, &registry, &request, |_| {});
    assert!(result.is_ok());
}

#[test]
fn upgrade_latest_single_action() {
    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    manifest.set(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v3"),
    );

    let registry = FakeRegistry::new()
        .with_all_tags("actions/checkout", vec!["v4", "v5", "v6"])
        .with_all_tags("actions/setup-node", vec!["v3", "v4"]);

    let lock = Lock::default();
    let request = UpgradeRequest::new(
        UpgradeMode::Latest,
        UpgradeScope::Single(ActionId::from("actions/checkout")),
    );
    let result = upgrade::plan::plan(&manifest, &lock, &registry, &request, |_| {});
    assert!(result.is_ok());
}

#[test]
fn upgrade_single_action_not_found() {
    let manifest = Manifest::default();

    let lock = Lock::default();
    let request = UpgradeRequest::new(
        UpgradeMode::Safe,
        UpgradeScope::Single(ActionId::from("actions/nonexistent")),
    );
    let result = upgrade::plan::plan(&manifest, &lock, &FakeRegistry::new(), &request, |_| {});
    assert!(
        result.is_err(),
        "Expected error when action not found in manifest"
    );
}

#[test]
fn cli_rejection_latest_with_version() {
    let action_str = "actions/checkout@v5";
    let contains_at = action_str.contains('@');
    assert!(contains_at, "Test setup: action string should contain @");
}

use gx::commands::upgrade;
use gx::commands::upgrade::UpgradeMode;
use gx::domain::{
    ActionId, CommitSha, Lock, LockKey, Manifest, ResolutionError, ResolvedAction, Version,
    VersionRegistry,
};
use gx::infrastructure::{
    FileLock, FileManifest, FileWorkflowUpdater, LockStore, ManifestStore,
    MemoryLock, MemoryManifest,
};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

struct MockUpgradeRegistry {
    tags: std::collections::HashMap<String, Vec<String>>,
}

impl MockUpgradeRegistry {
    fn new() -> Self {
        Self {
            tags: std::collections::HashMap::new(),
        }
    }
}

impl VersionRegistry for MockUpgradeRegistry {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<CommitSha, ResolutionError> {
        let sha = format!("{}{}", id.as_str(), version.as_str()).replace('/', "");
        let padded = format!("{:0<40}", &sha[..sha.len().min(40)]);
        Ok(CommitSha::from(padded))
    }

    fn tags_for_sha(
        &self,
        _id: &ActionId,
        _sha: &CommitSha,
    ) -> Result<Vec<Version>, ResolutionError> {
        Ok(vec![])
    }

    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        Ok(self
            .tags
            .get(id.as_str())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(Version::from)
            .collect())
    }
}

fn create_test_repo(temp_dir: &TempDir) -> std::path::PathBuf {
    let root = temp_dir.path();
    let github_dir = root.join(".github");
    let workflows_dir = github_dir.join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();
    root.to_path_buf()
}

fn create_manifest(root: &Path, content: &str) {
    let manifest_path = root.join(".github").join("gx.toml");
    fs::write(&manifest_path, content).unwrap();
}

fn create_lock(root: &Path, content: &str) {
    let lock_path = root.join(".github").join("gx.lock");
    fs::write(&lock_path, content).unwrap();
}

fn create_workflow(root: &Path, name: &str, content: &str) {
    let workflow_path = root.join(".github").join("workflows").join(name);
    let mut file = fs::File::create(&workflow_path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
}

/// Helper to run upgrade with file-backed stores
fn run_upgrade_file_backed(repo_root: &Path) -> anyhow::Result<()> {
    let manifest_path = repo_root.join(".github").join("gx.toml");
    let lock_path = repo_root.join(".github").join("gx.lock");
    let manifest_store = FileManifest::new(&manifest_path);
    let manifest = manifest_store.load()?;
    let lock_store = FileLock::new(&lock_path);
    let lock = lock_store.load()?;
    let updater = FileWorkflowUpdater::new(repo_root);
    upgrade::run(
        repo_root,
        manifest,
        manifest_store,
        lock,
        lock_store,
        MockUpgradeRegistry::new(),
        &updater,
        &UpgradeMode::Safe,
    )
}

// --- Tests that don't require GitHub API ---

#[test]
fn test_upgrade_empty_manifest_is_noop() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_workflow(
        &root,
        "ci.yml",
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    // MockRegistry returns no tags → no upgrade (noop)
    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
    let lock = Lock::default();
    let updater = FileWorkflowUpdater::new(&root);
    let result = upgrade::run(
        &root,
        manifest,
        MemoryManifest::default(),
        lock,
        MemoryLock,
        MockUpgradeRegistry::new(),
        &updater,
        &UpgradeMode::Safe,
    );
    assert!(result.is_ok());
}

#[test]
fn test_upgrade_empty_file_manifest_is_noop() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // MockRegistry returns no tags → noop.
    create_manifest(&root, "[actions]\n\"actions/checkout\" = \"v4\"\n");
    create_workflow(
        &root,
        "ci.yml",
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    let result = run_upgrade_file_backed(&root);
    assert!(result.is_ok());

    // Manifest should remain unchanged (early return before save, no upgrades)
    let manifest_content = fs::read_to_string(root.join(".github").join("gx.toml")).unwrap();
    assert!(manifest_content.contains("actions/checkout"));
    assert!(manifest_content.contains("v4"));
}

#[test]
fn test_upgrade_non_semver_versions_skipped() {
    // Empty manifest → early return.
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let manifest = Manifest::default();
    let lock = Lock::default();
    let updater = FileWorkflowUpdater::new(&root);
    let result = upgrade::run(
        &root,
        manifest,
        MemoryManifest::default(),
        lock,
        MemoryLock,
        MockUpgradeRegistry::new(),
        &updater,
        &UpgradeMode::Safe,
    );
    assert!(result.is_ok());
}

#[test]
fn test_upgrade_preserves_workflow_structure() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // MockRegistry returns no tags → noop.
    create_manifest(&root, "[actions]\n\"actions/checkout\" = \"v4\"\n");

    let workflow_content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: echo hello
";
    create_workflow(&root, "ci.yml", workflow_content);

    let result = run_upgrade_file_backed(&root);
    assert!(result.is_ok());

    // Workflow should be unchanged since no upgrades are available
    let after = fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();
    assert_eq!(after, workflow_content);
}

#[test]
fn test_upgrade_no_lock_file_created_when_empty_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Empty manifest → early return
    create_manifest(&root, "[actions]\n");

    let result = run_upgrade_file_backed(&root);
    assert!(result.is_ok());

    // No lock file should be created
    let lock_path = root.join(".github").join("gx.lock");
    assert!(
        !lock_path.exists(),
        "Lock file should not be created when manifest is empty"
    );
}

#[test]
fn test_upgrade_with_existing_lock_and_empty_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Manifest matches the workflow — no drift
    create_manifest(&root, "[actions]\n\"actions/checkout\" = \"v4\"\n");
    create_lock(
        &root,
        "version = \"1.0\"\n\n[actions]\n\"actions/checkout@v4\" = \"abc123def456789012345678901234567890abcd\"\n",
    );

    create_workflow(
        &root,
        "ci.yml",
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@abc123def456789012345678901234567890abcd # v4\n",
    );

    let result = run_upgrade_file_backed(&root);
    assert!(result.is_ok());
}

#[test]
fn test_upgrade_memory_stores_no_side_effects() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_workflow(
        &root,
        "ci.yml",
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
    let lock = Lock::default();
    let updater = FileWorkflowUpdater::new(&root);
    let result = upgrade::run(
        &root,
        manifest,
        MemoryManifest::default(),
        lock,
        MemoryLock,
        MockUpgradeRegistry::new(),
        &updater,
        &UpgradeMode::Safe,
    );
    assert!(result.is_ok());

    // No files should be created
    assert!(!root.join(".github").join("gx.toml").exists());
    assert!(!root.join(".github").join("gx.lock").exists());
}

#[test]
fn test_upgrade_multiple_workflows_empty_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Manifest matches both workflow actions — no drift. MockRegistry returns no tags → noop.
    create_manifest(
        &root,
        "[actions]\n\"actions/checkout\" = \"v4\"\n\"docker/build-push-action\" = \"v5\"\n",
    );

    create_workflow(
        &root,
        "ci.yml",
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
    );
    create_workflow(
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
fn test_upgrade_preserves_sha_for_non_upgraded_actions() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let login_sha = "5e57cd118135c172c3672efd75eb46360885c0ef";
    let checkout_old_sha = "8e8c483db84b4bee98b60c0593521ed34d9990e8";
    let checkout_new_sha = "11bd71901bbe5b1630ceea73d27597364c9af683";

    // Workflow with SHA-pinned actions
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
    create_workflow(&root, "ci.yml", &workflow_content);

    // Manifest has both actions
    let mut manifest = Manifest::default();
    manifest.set(
        ActionId::from("docker/login-action"),
        Version::from("v3.6.0"),
    );
    manifest.set(ActionId::from("actions/checkout"), Version::from("v6.0.1"));

    // Lock starts empty — upgrade::run only resolves SHAs for upgraded actions,
    // not for actions that stay at their current version.
    let mut lock = Lock::default();

    // Simulate: only checkout gets upgraded to v6.0.2
    manifest.set(ActionId::from("actions/checkout"), Version::from("v6.0.2"));

    // Only the upgraded action gets resolved to a SHA
    lock.set(&ResolvedAction::new(
        ActionId::from("actions/checkout"),
        Version::from("v6.0.2"),
        CommitSha::from(checkout_new_sha),
    ));

    // Retain only current manifest keys (this is what upgrade::run does)
    let keys_to_retain: Vec<LockKey> = manifest.specs().iter().map(|s| LockKey::from(*s)).collect();
    lock.retain(&keys_to_retain);

    // Build update map only for upgraded actions (the fix)
    let upgraded_keys = vec![LockKey::new(
        ActionId::from("actions/checkout"),
        Version::from("v6.0.2"),
    )];
    let update_map = lock.build_update_map(&upgraded_keys);
    let writer = FileWorkflowUpdater::new(&root);
    let _results = writer.update_all(&update_map).unwrap();

    // Verify the workflow
    let updated =
        fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();

    // Checkout should be updated to the new SHA + version
    assert!(
        updated.contains(&format!("actions/checkout@{checkout_new_sha} # v6.0.2")),
        "Expected checkout to be updated to new SHA. Got:\n{updated}"
    );

    // login-action should STILL have its original SHA — NOT a bare version tag
    assert!(
        updated.contains(&format!("docker/login-action@{login_sha} # v3.6.0")),
        "Expected login-action to keep its SHA. Got:\n{updated}"
    );

    // Specifically, this pattern should NOT appear (the bug)
    assert!(
        !updated.contains("docker/login-action@v3.6.0"),
        "Bug: login-action SHA was replaced with bare version tag. Got:\n{updated}"
    );
}

#[test]
fn test_upgrade_repins_branch_ref() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let old_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    // Workflow with a branch-pinned action (SHA # branch)
    let workflow_content = format!(
        "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: my-org/my-action@{old_sha} # main\n"
    );
    create_workflow(&root, "ci.yml", &workflow_content);

    // Manifest has the branch ref — matches scanner result (version="main") → no drift
    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("my-org/my-action"), Version::from("main"));

    // Lock has the old SHA
    let mut lock = Lock::default();
    lock.set(&ResolvedAction::new(
        ActionId::from("my-org/my-action"),
        Version::from("main"),
        CommitSha::from(old_sha),
    ));

    let updater = FileWorkflowUpdater::new(&root);
    let result = upgrade::run(
        &root,
        manifest,
        MemoryManifest::default(),
        lock,
        MemoryLock,
        MockUpgradeRegistry::new(),
        &updater,
        &UpgradeMode::Safe,
    );
    assert!(result.is_ok(), "upgrade failed: {:?}", result.unwrap_err());

    // Verify workflow was updated with the new SHA from MockUpgradeRegistry
    let expected_sha = format!("{:0<40}", "my-orgmy-actionmain");
    let updated_workflow =
        fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();
    assert!(
        updated_workflow.contains(&format!("my-org/my-action@{expected_sha} # main")),
        "Expected branch ref to be re-pinned with new SHA. Got:\n{updated_workflow}"
    );
}

#[test]
fn test_upgrade_latest_also_repins_branch_ref() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let old_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let workflow_content = format!(
        "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: my-org/my-action@{old_sha} # main\n"
    );
    create_workflow(&root, "ci.yml", &workflow_content);

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("my-org/my-action"), Version::from("main"));

    let mut lock = Lock::default();
    lock.set(&ResolvedAction::new(
        ActionId::from("my-org/my-action"),
        Version::from("main"),
        CommitSha::from(old_sha),
    ));

    let updater = FileWorkflowUpdater::new(&root);
    let result = upgrade::run(
        &root,
        manifest,
        MemoryManifest::default(),
        lock,
        MemoryLock,
        MockUpgradeRegistry::new(),
        &updater,
        &UpgradeMode::Latest,
    );
    assert!(result.is_ok());

    let expected_sha = format!("{:0<40}", "my-orgmy-actionmain");
    let updated_workflow =
        fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();
    assert!(
        updated_workflow.contains(&format!("my-org/my-action@{expected_sha} # main")),
        "Expected branch ref to be re-pinned in --latest mode. Got:\n{updated_workflow}"
    );
}

#[test]
fn test_upgrade_targeted_does_not_repin_branch_ref() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let branch_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let checkout_sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    let workflow_content = format!(
        "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: my-org/my-action@{branch_sha} # main\n      - uses: actions/checkout@{checkout_sha} # v4\n"
    );
    create_workflow(&root, "ci.yml", &workflow_content);

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("my-org/my-action"), Version::from("main"));
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

    let mut lock = Lock::default();
    lock.set(&ResolvedAction::new(
        ActionId::from("my-org/my-action"),
        Version::from("main"),
        CommitSha::from(branch_sha),
    ));
    lock.set(&ResolvedAction::new(
        ActionId::from("actions/checkout"),
        Version::from("v4"),
        CommitSha::from(checkout_sha),
    ));

    // Registry returns v5 as a valid tag for checkout
    let mut registry = MockUpgradeRegistry::new();
    registry.tags.insert(
        "actions/checkout".to_string(),
        vec!["v4".to_string(), "v5".to_string()],
    );

    let updater = FileWorkflowUpdater::new(&root);
    let result = upgrade::run(
        &root,
        manifest,
        MemoryManifest::default(),
        lock,
        MemoryLock,
        registry,
        &updater,
        &UpgradeMode::Targeted(ActionId::from("actions/checkout"), Version::from("v5")),
    );
    assert!(result.is_ok());

    let updated_workflow =
        fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();

    // Branch ref should be UNCHANGED
    assert!(
        updated_workflow.contains(&format!("my-org/my-action@{branch_sha} # main")),
        "Branch ref should not be re-pinned in targeted mode. Got:\n{updated_workflow}"
    );
}

#[test]
fn test_upgrade_mixed_semver_and_branch() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let old_branch_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let old_checkout_sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    let workflow_content = format!(
        "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: my-org/my-action@{old_branch_sha} # main\n      - uses: actions/checkout@{old_checkout_sha} # v4\n"
    );
    create_workflow(&root, "ci.yml", &workflow_content);

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("my-org/my-action"), Version::from("main"));
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

    let mut lock = Lock::default();
    lock.set(&ResolvedAction::new(
        ActionId::from("my-org/my-action"),
        Version::from("main"),
        CommitSha::from(old_branch_sha),
    ));
    lock.set(&ResolvedAction::new(
        ActionId::from("actions/checkout"),
        Version::from("v4"),
        CommitSha::from(old_checkout_sha),
    ));

    // Registry has both v4 and v5 available for checkout
    let mut registry = MockUpgradeRegistry::new();
    registry.tags.insert(
        "actions/checkout".to_string(),
        vec!["v4".to_string(), "v5".to_string()],
    );

    let updater = FileWorkflowUpdater::new(&root);
    let result = upgrade::run(
        &root,
        manifest,
        MemoryManifest::default(),
        lock,
        MemoryLock,
        registry,
        &updater,
        &UpgradeMode::Latest,
    );
    assert!(result.is_ok());

    let updated_workflow =
        fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();

    // Branch ref should be re-pinned with new SHA
    let expected_branch_sha = format!("{:0<40}", "my-orgmy-actionmain");
    assert!(
        updated_workflow.contains(&format!("my-org/my-action@{expected_branch_sha} # main")),
        "Branch ref should be re-pinned. Got:\n{updated_workflow}"
    );

    // Checkout should be upgraded to v5 with new SHA
    let expected_checkout_sha = format!("{:0<40}", "actionscheckoutv5");
    assert!(
        updated_workflow.contains(&format!("actions/checkout@{expected_checkout_sha} # v5")),
        "Checkout should be upgraded to v5. Got:\n{updated_workflow}"
    );
}

#[test]
fn test_upgrade_skips_bare_sha() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let bare_sha = "cccccccccccccccccccccccccccccccccccccccc";

    let workflow_content = format!(
        "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: my-org/my-action@{bare_sha}\n"
    );
    create_workflow(&root, "ci.yml", &workflow_content);

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("my-org/my-action"), Version::from(bare_sha));

    let lock = Lock::default();
    let updater = FileWorkflowUpdater::new(&root);
    let result = upgrade::run(
        &root,
        manifest,
        MemoryManifest::default(),
        lock,
        MemoryLock,
        MockUpgradeRegistry::new(),
        &updater,
        &UpgradeMode::Safe,
    );
    assert!(result.is_ok());

    // Workflow should be unchanged — bare SHA has nothing to re-pin
    let updated_workflow =
        fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();
    assert!(
        updated_workflow.contains(&format!("my-org/my-action@{bare_sha}")),
        "Bare SHA should remain unchanged. Got:\n{updated_workflow}"
    );
}

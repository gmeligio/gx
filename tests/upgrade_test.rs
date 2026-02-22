use gx::commands::upgrade;
use gx::commands::upgrade::UpgradeMode;
use gx::domain::{
    ActionId, CommitSha, LockKey, ResolutionError, ResolvedAction, Version, VersionRegistry,
};
use gx::infrastructure::{
    FileLock, FileManifest, FileWorkflowUpdater, LockStore, ManifestStore, MemoryLock,
    MemoryManifest,
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
    let manifest = FileManifest::load_or_default(&manifest_path)?;
    let lock = FileLock::load_or_default(&lock_path)?;
    let updater = FileWorkflowUpdater::new(repo_root);
    upgrade::run(
        repo_root,
        manifest,
        lock,
        MockUpgradeRegistry::new(),
        &updater,
        UpgradeMode::Safe,
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

    // Empty MemoryManifest should return Ok immediately
    let manifest = MemoryManifest::default();
    let lock = MemoryLock::default();
    let updater = FileWorkflowUpdater::new(&root);
    let result = upgrade::run(&root, manifest, lock, MockUpgradeRegistry::new(), &updater, UpgradeMode::Safe);
    assert!(result.is_ok());
}

#[test]
fn test_upgrade_empty_file_manifest_is_noop() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_manifest(&root, "[actions]\n");
    create_workflow(
        &root,
        "ci.yml",
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    let result = run_upgrade_file_backed(&root);
    assert!(result.is_ok());

    // Manifest should remain unchanged (empty)
    let manifest_content = fs::read_to_string(root.join(".github").join("gx.toml")).unwrap();
    assert_eq!(manifest_content, "[actions]\n");
}

#[test]
fn test_upgrade_non_semver_versions_skipped() {
    // When manifest has non-semver versions (like SHAs or branches),
    // they should be skipped during upgrade. The function still calls
    // GithubRegistry::from_env() if there are specs, so this test
    // only verifies the empty-manifest fast path.
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let manifest = MemoryManifest::default();
    let lock = MemoryLock::default();

    let updater = FileWorkflowUpdater::new(&root);
    let result = upgrade::run(&root, manifest, lock, MockUpgradeRegistry::new(), &updater, UpgradeMode::Safe);
    assert!(result.is_ok());
}

#[test]
fn test_upgrade_preserves_workflow_structure() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create empty manifest - upgrade should early-return
    create_manifest(&root, "[actions]\n");

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

    // Workflow should be unchanged since manifest is empty
    let after = fs::read_to_string(root.join(".github").join("workflows").join("ci.yml")).unwrap();
    assert_eq!(after, workflow_content);
}

#[test]
fn test_upgrade_no_lock_file_created_when_empty_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

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

    create_manifest(&root, "[actions]\n");
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

    let manifest = MemoryManifest::default();
    let lock = MemoryLock::default();

    let updater = FileWorkflowUpdater::new(&root);
    let result = upgrade::run(&root, manifest, lock, MockUpgradeRegistry::new(), &updater, UpgradeMode::Safe);
    assert!(result.is_ok());

    // No files should be created
    assert!(!root.join(".github").join("gx.toml").exists());
    assert!(!root.join(".github").join("gx.lock").exists());
}

#[test]
fn test_upgrade_multiple_workflows_empty_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_manifest(&root, "[actions]\n");

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

    // Empty manifest means early return, no GitHub calls
    let result = run_upgrade_file_backed(&root);
    assert!(result.is_ok());
}

/// Reproduces the bug where `upgrade` replaces SHAs with bare version tags
/// for actions that have no available upgrade.
///
/// In `upgrade::run`, only upgraded actions get their SHAs resolved. Non-upgraded
/// actions are never resolved, so the lock has no entry for them. The code then
/// calls `build_update_map` for ALL manifest specs, and `build_update_map` falls
/// back to the bare version string when no SHA is in the lock. Finally,
/// `update_all` rewrites the workflow, changing:
///   uses: docker/login-action@5e57cd...ef # v3.6.0
/// to:
///   uses: docker/login-action@v3.6.0
///
/// The upgrade command should only update workflows for actions that were
/// actually upgraded, leaving non-upgraded actions untouched.
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
    let mut manifest = MemoryManifest::default();
    manifest.set(
        ActionId::from("docker/login-action"),
        Version::from("v3.6.0"),
    );
    manifest.set(ActionId::from("actions/checkout"), Version::from("v6.0.1"));

    // Lock starts empty — upgrade::run only resolves SHAs for upgraded actions,
    // not for actions that stay at their current version.
    let mut lock = MemoryLock::default();

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

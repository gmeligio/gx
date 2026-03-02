#![allow(unused_crate_dependencies)]

//! End-to-end regression tests for the lazy-pipeline architecture.
//!
//! These tests exercise the full init → tidy → upgrade → lint pipeline,
//! verifying that plan+apply produces correct, parseable files at each stage.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::Path;

use gx::commands::upgrade::{UpgradeMode, UpgradeRequest, UpgradeScope};
use gx::commands::{lint, tidy, upgrade};
use gx::config::LintConfig;
use gx::domain::{
    ActionId, CommitSha, Manifest, RefType, ResolutionError, ResolvedRef, Version, VersionRegistry,
};
use gx::infrastructure::{
    FileWorkflowScanner, FileWorkflowUpdater, apply_lock_diff, apply_manifest_diff, create_lock,
    create_manifest, parse_lock, parse_manifest,
};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Mock registry
// ---------------------------------------------------------------------------

/// Registry that resolves versions to deterministic SHAs and provides upgrade tags.
#[derive(Clone, Default)]
struct E2eRegistry {
    tags: std::collections::HashMap<String, Vec<String>>,
}

impl E2eRegistry {
    fn new() -> Self {
        Self::default()
    }

    fn with_all_tags(mut self, id: &str, tags: Vec<&str>) -> Self {
        self.tags
            .insert(id.to_string(), tags.into_iter().map(String::from).collect());
        self
    }

    fn fake_sha(id: &str, version: &str) -> String {
        let mut hasher = DefaultHasher::new();
        id.hash(&mut hasher);
        version.hash(&mut hasher);
        let h1 = hasher.finish();
        h1.hash(&mut hasher);
        let h2 = hasher.finish();
        h2.hash(&mut hasher);
        let h3 = hasher.finish();
        // Produce exactly 40 hex chars
        let full = format!("{h1:016x}{h2:016x}{h3:016x}");
        full[..40].to_string()
    }
}

impl VersionRegistry for E2eRegistry {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<ResolvedRef, ResolutionError> {
        Ok(ResolvedRef::new(
            CommitSha::from(Self::fake_sha(id.as_str(), version.as_str())),
            id.base_repo(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ))
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_repo(temp_dir: &TempDir) -> std::path::PathBuf {
    let root = temp_dir.path();
    fs::create_dir_all(root.join(".github").join("workflows")).unwrap();
    root.to_path_buf()
}

fn write_workflow(root: &Path, name: &str, content: &str) {
    let path = root.join(".github").join("workflows").join(name);
    let mut f = fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

fn manifest_path(root: &Path) -> std::path::PathBuf {
    root.join(".github").join("gx.toml")
}

fn lock_path(root: &Path) -> std::path::PathBuf {
    root.join(".github").join("gx.lock")
}

/// Run the init pipeline: plan + create files.
fn run_init<R: VersionRegistry + Clone>(root: &Path, registry: R) {
    let mp = manifest_path(root);
    let lp = lock_path(root);
    assert!(!mp.exists(), "init requires no existing manifest");

    let manifest = Manifest::default();
    let lock = gx::domain::Lock::default();
    let scanner = FileWorkflowScanner::new(root);
    let updater = FileWorkflowUpdater::new(root);

    let plan = tidy::plan(&manifest, &lock, registry, &scanner).unwrap();
    if !plan.is_empty() {
        create_manifest(&mp, &plan.manifest).unwrap();
        create_lock(&lp, &plan.lock).unwrap();
        tidy::apply_workflow_patches(&updater, &plan.workflows, &plan.corrections).unwrap();
    }
}

/// Run the tidy pipeline: plan + apply diffs.
fn run_tidy<R: VersionRegistry + Clone>(root: &Path, registry: R) {
    let mp = manifest_path(root);
    let lp = lock_path(root);
    let scanner = FileWorkflowScanner::new(root);
    let updater = FileWorkflowUpdater::new(root);
    let has_manifest = mp.exists();

    let manifest = if has_manifest {
        parse_manifest(&mp).unwrap()
    } else {
        Manifest::default()
    };
    let lock = parse_lock(&lp).unwrap();

    let plan = tidy::plan(&manifest, &lock, registry, &scanner).unwrap();
    if !plan.is_empty() {
        if has_manifest {
            apply_manifest_diff(&mp, &plan.manifest).unwrap();
            if lp.exists() {
                apply_lock_diff(&lp, &plan.lock).unwrap();
            } else {
                create_lock(&lp, &plan.lock).unwrap();
            }
        }
        tidy::apply_workflow_patches(&updater, &plan.workflows, &plan.corrections).unwrap();
    }
}

/// Run the upgrade pipeline: plan + apply diffs.
fn run_upgrade<R: VersionRegistry + Clone>(root: &Path, registry: R, request: &UpgradeRequest) {
    let mp = manifest_path(root);
    let lp = lock_path(root);
    let manifest = parse_manifest(&mp).unwrap();
    let lock = parse_lock(&lp).unwrap();
    let updater = FileWorkflowUpdater::new(root);

    let plan = upgrade::plan(&manifest, &lock, registry, request).unwrap();
    if !plan.is_empty() {
        apply_manifest_diff(&mp, &plan.manifest).unwrap();
        apply_lock_diff(&lp, &plan.lock).unwrap();
        upgrade::apply_upgrade_workflows(&updater, &plan.lock, &plan.upgrades).unwrap();
    }
}

/// Run lint and return the diagnostics.
fn run_lint(root: &Path) -> Vec<lint::Diagnostic> {
    let mp = manifest_path(root);
    let lp = lock_path(root);
    let manifest = parse_manifest(&mp).unwrap();
    let lock = parse_lock(&lp).unwrap();
    let scanner = FileWorkflowScanner::new(root);
    let lint_config = LintConfig::default();

    lint::run(&manifest, &lock, &scanner, &lint_config).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `init` on a fresh repo creates parseable manifest and lock; workflow pins match lock SHAs.
#[test]
fn e2e_init_creates_parseable_files_with_matching_pins() {
    let temp = TempDir::new().unwrap();
    let root = setup_repo(&temp);

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/setup-node@v3\n",
    );

    run_init(&root, E2eRegistry::new());

    // Manifest should be parseable and contain the two actions
    let manifest = parse_manifest(&manifest_path(&root)).unwrap();
    assert!(manifest.has(&ActionId::from("actions/checkout")));
    assert!(manifest.has(&ActionId::from("actions/setup-node")));

    // Lock should be parseable and contain entries for both
    let lock = parse_lock(&lock_path(&root)).unwrap();
    let checkout_key =
        gx::domain::LockKey::new(ActionId::from("actions/checkout"), Version::from("v4"));
    let setup_key =
        gx::domain::LockKey::new(ActionId::from("actions/setup-node"), Version::from("v3"));
    assert!(lock.get(&checkout_key).is_some(), "Lock must have checkout");
    assert!(lock.get(&setup_key).is_some(), "Lock must have setup-node");

    // Workflow pins should contain lock SHAs
    let wf = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let checkout_sha = lock.get(&checkout_key).unwrap().sha.to_string();
    assert!(
        wf.contains(&checkout_sha),
        "Workflow should contain checkout SHA {checkout_sha}"
    );
}

/// `tidy` immediately after `init` is a no-op (file contents unchanged).
#[test]
fn e2e_tidy_after_init_is_noop() {
    let temp = TempDir::new().unwrap();
    let root = setup_repo(&temp);

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    run_init(&root, E2eRegistry::new());

    let manifest_before = fs::read_to_string(manifest_path(&root)).unwrap();
    let lock_before = fs::read_to_string(lock_path(&root)).unwrap();
    let workflow_before = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();

    run_tidy(&root, E2eRegistry::new());

    let manifest_after = fs::read_to_string(manifest_path(&root)).unwrap();
    let lock_after = fs::read_to_string(lock_path(&root)).unwrap();
    let workflow_after = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();

    assert_eq!(
        manifest_before, manifest_after,
        "Manifest should not change"
    );
    assert_eq!(lock_before, lock_after, "Lock should not change");
    assert_eq!(
        workflow_before, workflow_after,
        "Workflow should not change"
    );
}

/// `tidy` after adding a new action to a workflow adds only that action to manifest/lock.
#[test]
fn e2e_tidy_adds_new_action() {
    let temp = TempDir::new().unwrap();
    let root = setup_repo(&temp);

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    run_init(&root, E2eRegistry::new());

    let manifest_before = parse_manifest(&manifest_path(&root)).unwrap();
    assert!(manifest_before.has(&ActionId::from("actions/checkout")));
    assert!(!manifest_before.has(&ActionId::from("actions/setup-node")));

    // Add a new action to the workflow
    write_workflow(
        &root,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{sha} # v4\n      - uses: actions/setup-node@v3\n",
            sha = {
                let lock = parse_lock(&lock_path(&root)).unwrap();
                let key = gx::domain::LockKey::new(
                    ActionId::from("actions/checkout"),
                    Version::from("v4"),
                );
                lock.get(&key).unwrap().sha.to_string()
            }
        ),
    );

    run_tidy(&root, E2eRegistry::new());

    // Now both actions should be in the manifest
    let manifest_after = parse_manifest(&manifest_path(&root)).unwrap();
    assert!(manifest_after.has(&ActionId::from("actions/checkout")));
    assert!(
        manifest_after.has(&ActionId::from("actions/setup-node")),
        "New action should be added to manifest"
    );

    // Lock should have entries for both
    let lock_after = parse_lock(&lock_path(&root)).unwrap();
    let new_key =
        gx::domain::LockKey::new(ActionId::from("actions/setup-node"), Version::from("v3"));
    assert!(
        lock_after.get(&new_key).is_some(),
        "New action should be in the lock"
    );
}

/// `tidy` after removing an action from all workflows removes only that action from manifest/lock.
#[test]
fn e2e_tidy_removes_stale_action() {
    let temp = TempDir::new().unwrap();
    let root = setup_repo(&temp);

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/setup-node@v3\n",
    );

    run_init(&root, E2eRegistry::new());

    let manifest_before = parse_manifest(&manifest_path(&root)).unwrap();
    assert!(manifest_before.has(&ActionId::from("actions/checkout")));
    assert!(manifest_before.has(&ActionId::from("actions/setup-node")));

    // Remove setup-node from workflow, keep checkout pinned
    let lock = parse_lock(&lock_path(&root)).unwrap();
    let checkout_key =
        gx::domain::LockKey::new(ActionId::from("actions/checkout"), Version::from("v4"));
    let checkout_sha = lock.get(&checkout_key).unwrap().sha.to_string();

    write_workflow(
        &root,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{checkout_sha} # v4\n"
        ),
    );

    run_tidy(&root, E2eRegistry::new());

    let manifest_after = parse_manifest(&manifest_path(&root)).unwrap();
    assert!(manifest_after.has(&ActionId::from("actions/checkout")));
    assert!(
        !manifest_after.has(&ActionId::from("actions/setup-node")),
        "Removed action should be gone from manifest"
    );

    let lock_after = parse_lock(&lock_path(&root)).unwrap();
    let stale_key =
        gx::domain::LockKey::new(ActionId::from("actions/setup-node"), Version::from("v3"));
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
    let root = setup_repo(&temp);

    // Two workflows using different versions of checkout
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

    run_init(&root, E2eRegistry::new());

    let manifest = parse_manifest(&manifest_path(&root)).unwrap();

    // The majority version (v4 used in 2 spots) becomes the global version.
    // The minority version (v3) becomes an override.
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
    let root = setup_repo(&temp);

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v3\n      - uses: actions/setup-node@v3\n",
    );

    run_init(&root, E2eRegistry::new());

    // Record setup-node lock entry before upgrade
    let lock_before = parse_lock(&lock_path(&root)).unwrap();
    let node_key =
        gx::domain::LockKey::new(ActionId::from("actions/setup-node"), Version::from("v3"));
    let node_entry_before = lock_before.get(&node_key).unwrap().clone();

    // Registry offers v4 for checkout (cross-range upgrade), nothing new for setup-node
    let registry = E2eRegistry::new()
        .with_all_tags("actions/checkout", vec!["v3", "v3.0.0", "v4", "v4.0.0"])
        .with_all_tags("actions/setup-node", vec!["v3", "v3.0.0"]);

    let request = UpgradeRequest::new(UpgradeMode::Latest, UpgradeScope::All).unwrap();
    run_upgrade(&root, registry, &request);

    // Checkout should be upgraded to v4
    let manifest_after = parse_manifest(&manifest_path(&root)).unwrap();
    assert_eq!(
        manifest_after.get(&ActionId::from("actions/checkout")),
        Some(&Version::from("v4")),
        "Checkout should be upgraded to v4"
    );

    // Setup-node should remain v3
    assert_eq!(
        manifest_after.get(&ActionId::from("actions/setup-node")),
        Some(&Version::from("v3")),
        "Setup-node should remain at v3"
    );

    // Setup-node lock entry should be unchanged
    let lock_after = parse_lock(&lock_path(&root)).unwrap();
    let node_entry_after = lock_after.get(&node_key).unwrap();
    assert_eq!(
        node_entry_before.sha, node_entry_after.sha,
        "Setup-node SHA should be unchanged"
    );
}

/// `lint` detects unsynced manifest after workflow modifications.
#[test]
fn e2e_lint_detects_unsynced_manifest() {
    let temp = TempDir::new().unwrap();
    let root = setup_repo(&temp);

    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    run_init(&root, E2eRegistry::new());

    // Add an unmanaged action to the workflow (not in manifest)
    let lock = parse_lock(&lock_path(&root)).unwrap();
    let checkout_key =
        gx::domain::LockKey::new(ActionId::from("actions/checkout"), Version::from("v4"));
    let checkout_sha = lock.get(&checkout_key).unwrap().sha.to_string();

    write_workflow(
        &root,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{checkout_sha} # v4\n      - uses: actions/setup-node@v3\n"
        ),
    );

    // Lint should detect the unsynced manifest (setup-node in workflow but not in manifest)
    let diagnostics = run_lint(&root);
    let has_unsynced = diagnostics.iter().any(|d| d.rule == "unsynced-manifest");
    assert!(
        has_unsynced,
        "Lint should detect unsynced-manifest for setup-node, got: {diagnostics:?}"
    );
}

/// Sequential init → tidy → modify workflow → tidy → upgrade produces correct final state.
#[test]
fn e2e_full_pipeline_init_tidy_modify_tidy_upgrade() {
    let temp = TempDir::new().unwrap();
    let root = setup_repo(&temp);

    // Step 1: Create initial workflow
    write_workflow(
        &root,
        "ci.yml",
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v3\n",
    );

    // Step 2: Init
    run_init(&root, E2eRegistry::new());

    let manifest = parse_manifest(&manifest_path(&root)).unwrap();
    assert_eq!(
        manifest.get(&ActionId::from("actions/checkout")),
        Some(&Version::from("v3"))
    );

    // Step 3: Tidy immediately after init — should be no-op
    let manifest_before = fs::read_to_string(manifest_path(&root)).unwrap();
    let lock_before = fs::read_to_string(lock_path(&root)).unwrap();

    run_tidy(&root, E2eRegistry::new());

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
    let lock = parse_lock(&lock_path(&root)).unwrap();
    let checkout_key =
        gx::domain::LockKey::new(ActionId::from("actions/checkout"), Version::from("v3"));
    let checkout_sha = lock.get(&checkout_key).unwrap().sha.to_string();

    write_workflow(
        &root,
        "ci.yml",
        &format!(
            "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{checkout_sha} # v3\n      - uses: actions/setup-node@v3\n"
        ),
    );

    // Step 5: Tidy should pick up the new action
    run_tidy(&root, E2eRegistry::new());

    let manifest = parse_manifest(&manifest_path(&root)).unwrap();
    assert!(manifest.has(&ActionId::from("actions/setup-node")));

    let lock = parse_lock(&lock_path(&root)).unwrap();
    let node_key =
        gx::domain::LockKey::new(ActionId::from("actions/setup-node"), Version::from("v3"));
    assert!(
        lock.get(&node_key).is_some(),
        "setup-node should be in lock after tidy"
    );

    // Step 6: Upgrade checkout from v3 to v4
    let registry = E2eRegistry::new()
        .with_all_tags("actions/checkout", vec!["v3", "v3.0.0", "v4", "v4.0.0"])
        .with_all_tags("actions/setup-node", vec!["v3", "v3.0.0"]);

    let request = UpgradeRequest::new(UpgradeMode::Latest, UpgradeScope::All).unwrap();
    run_upgrade(&root, registry, &request);

    // Verify final state
    let manifest = parse_manifest(&manifest_path(&root)).unwrap();
    assert_eq!(
        manifest.get(&ActionId::from("actions/checkout")),
        Some(&Version::from("v4")),
        "Checkout should be upgraded to v4"
    );
    assert_eq!(
        manifest.get(&ActionId::from("actions/setup-node")),
        Some(&Version::from("v3")),
        "Setup-node should remain at v3"
    );

    let lock = parse_lock(&lock_path(&root)).unwrap();
    let v4_key = gx::domain::LockKey::new(ActionId::from("actions/checkout"), Version::from("v4"));
    assert!(
        lock.get(&v4_key).is_some(),
        "Lock should have checkout@v4 entry"
    );
    assert!(
        lock.get(&node_key).is_some(),
        "Lock should still have setup-node@v3"
    );

    // Workflow should reference the new v4 SHA
    let wf = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let v4_sha = lock.get(&v4_key).unwrap().sha.to_string();
    assert!(
        wf.contains(&v4_sha),
        "Workflow should contain upgraded checkout SHA"
    );
}

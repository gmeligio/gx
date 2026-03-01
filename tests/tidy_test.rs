#![allow(unused_crate_dependencies)]
use gx::commands::{self, tidy};
use gx::domain::{
    ActionId, CommitSha, Lock, Manifest, RefType, ResolutionError, ResolvedRef, Version,
    VersionRegistry,
};
use gx::infrastructure::{
    FileLock, FileManifest, FileWorkflowScanner, FileWorkflowUpdater, parse_lock, parse_manifest,
};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::Path;
use std::string::ToString;
use tempfile::TempDir;

/// A no-op registry that always fails resolution (simulates missing `GITHUB_TOKEN`).
#[derive(Clone, Copy)]
struct NoopRegistry;

impl VersionRegistry for NoopRegistry {
    fn lookup_sha(
        &self,
        _id: &ActionId,
        _version: &Version,
    ) -> Result<ResolvedRef, ResolutionError> {
        Err(ResolutionError::TokenRequired)
    }

    fn tags_for_sha(
        &self,
        _id: &ActionId,
        _sha: &CommitSha,
    ) -> Result<Vec<Version>, ResolutionError> {
        Err(ResolutionError::TokenRequired)
    }

    fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        Err(ResolutionError::TokenRequired)
    }
}

/// A mock registry that resolves any version to a deterministic SHA
/// and tracks mappings so `tags_for_sha` returns consistent results.
#[derive(Clone, Default)]
struct MockRegistry {
    /// Maps (action, SHA) → list of version tags pointing to that SHA.
    sha_tags: std::collections::HashMap<(String, String), Vec<String>>,
}

impl MockRegistry {
    fn new() -> Self {
        Self::default()
    }

    /// Register that a SHA points to specific version tags.
    fn with_tags(mut self, id: &str, sha: &str, tags: &[&str]) -> Self {
        self.sha_tags.insert(
            (id.to_string(), sha.to_string()),
            tags.iter().map(ToString::to_string).collect(),
        );
        self
    }

    /// Generate a deterministic fake SHA from action id and version.
    fn fake_sha(id: &str, version: &str) -> String {
        let mut hasher = DefaultHasher::new();
        id.hash(&mut hasher);
        version.hash(&mut hasher);
        let hash = hasher.finish();
        format!("{hash:016x}{hash:016x}{hash:08x}")
    }
}

impl VersionRegistry for MockRegistry {
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
        id: &ActionId,
        sha: &CommitSha,
    ) -> Result<Vec<Version>, ResolutionError> {
        let key = (id.as_str().to_string(), sha.as_str().to_string());
        Ok(self
            .sha_tags
            .get(&key)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(Version::from)
            .collect())
    }

    fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        Ok(vec![])
    }
}

fn create_test_repo(temp_dir: &TempDir) -> std::path::PathBuf {
    let root = temp_dir.path();
    let github_dir = root.join(".github");
    let workflows_dir = github_dir.join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();
    root.to_path_buf()
}

/// Helper to run tidy with appropriate DI based on manifest existence (same logic as app.rs)
fn run_tidy(repo_root: &Path) -> Result<(), commands::app::AppError> {
    let manifest_path = repo_root.join(".github").join("gx.toml");
    let lock_path = repo_root.join(".github").join("gx.lock");
    let scanner = FileWorkflowScanner::new(repo_root);
    let updater = FileWorkflowUpdater::new(repo_root);

    if manifest_path.exists() {
        let manifest = parse_manifest(&manifest_path)?;
        let lock = parse_lock(&lock_path)?;
        let (updated_manifest, updated_lock) = tidy::run(
            manifest,
            lock,
            &manifest_path,
            MockRegistry::new(),
            &scanner,
            &updater,
        )?;
        FileManifest::new(&manifest_path).save(&updated_manifest)?;
        FileLock::new(&lock_path).save(&updated_lock)?;
    } else {
        let _ = tidy::run(
            Manifest::default(),
            Lock::default(),
            &manifest_path,
            MockRegistry::new(),
            &scanner,
            &updater,
        )?;
    }
    Ok(())
}

/// Helper to create an empty manifest file (triggers file-backed mode)
fn create_empty_manifest(root: &Path) {
    let manifest_path = root.join(".github").join("gx.toml");
    fs::write(&manifest_path, "[actions]\n").unwrap();
}

#[test]
fn test_gx_tidy_memory_only_mode_no_manifest_created() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create workflow without manifest
    let workflow_content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command (memory-only mode since no manifest exists)
    let result = run_tidy(&root);
    assert!(result.is_ok());

    // Verify NO manifest was created (memory-only mode)
    let manifest_path = root.join(".github").join("gx.toml");
    assert!(
        !manifest_path.exists(),
        "Manifest should not be created in memory-only mode"
    );

    // Verify NO lock file was created
    let lock_path = root.join(".github").join("gx.lock");
    assert!(
        !lock_path.exists(),
        "Lock file should not be created in memory-only mode"
    );

    // Workflow should be updated with resolved SHAs even in memory-only mode
    let workflow_content_after = fs::read_to_string(&workflow_path).unwrap();
    let checkout_sha = MockRegistry::fake_sha("actions/checkout", "v4");
    let node_sha = MockRegistry::fake_sha("actions/setup-node", "v3");
    assert!(
        workflow_content_after.contains(&format!("actions/checkout@{checkout_sha} # v4")),
        "Expected checkout SHA in workflow, got:\n{workflow_content_after}"
    );
    assert!(
        workflow_content_after.contains(&format!("actions/setup-node@{node_sha} # v3")),
        "Expected setup-node SHA in workflow, got:\n{workflow_content_after}"
    );
}

#[test]
fn test_gx_tidy_file_mode_creates_manifest_from_workflows() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create empty manifest to trigger file-backed mode
    create_empty_manifest(&root);

    // Create workflow
    let workflow_content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command (file-backed mode since manifest exists)
    let result = run_tidy(&root);
    assert!(result.is_ok());

    // Verify manifest was updated
    let manifest_path = root.join(".github").join("gx.toml");
    assert!(manifest_path.exists());

    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_content.contains("actions/checkout"));
    assert!(manifest_content.contains("v4"));
    assert!(manifest_content.contains("actions/setup-node"));
    assert!(manifest_content.contains("v3"));
}

#[test]
fn test_gx_tidy_updates_workflows_from_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create manifest
    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v4"
"#;
    let manifest_path = root.join(".github").join("gx.toml");
    let mut manifest_file = fs::File::create(&manifest_path).unwrap();
    manifest_file
        .write_all(manifest_content.as_bytes())
        .unwrap();

    // Create workflow with older versions
    let workflow_content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command
    let result = run_tidy(&root);
    assert!(result.is_ok());

    // Verify workflow was updated with SHAs from the manifest versions
    let updated_content = fs::read_to_string(&workflow_path).unwrap();
    let checkout_sha = MockRegistry::fake_sha("actions/checkout", "v4");
    let node_sha = MockRegistry::fake_sha("actions/setup-node", "v4");
    assert!(
        updated_content.contains(&format!("actions/checkout@{checkout_sha} # v4")),
        "Expected checkout with SHA, got:\n{updated_content}"
    );
    assert!(
        updated_content.contains(&format!("actions/setup-node@{node_sha} # v4")),
        "Expected setup-node with SHA, got:\n{updated_content}"
    );
    assert!(!updated_content.contains("@v3"));
}

#[test]
fn test_gx_tidy_removes_unused_actions() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create manifest with an action not used in workflows
    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"
"actions/unused-action" = "v1"
"#;
    let manifest_path = root.join(".github").join("gx.toml");
    let mut manifest_file = fs::File::create(&manifest_path).unwrap();
    manifest_file
        .write_all(manifest_content.as_bytes())
        .unwrap();

    // Create workflow that only uses checkout
    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command
    let result = run_tidy(&root);
    assert!(result.is_ok());

    // Verify unused action was removed from manifest
    let updated_manifest = fs::read_to_string(&manifest_path).unwrap();
    assert!(updated_manifest.contains("actions/checkout"));
    assert!(!updated_manifest.contains("actions/unused-action"));
}

#[test]
fn test_gx_tidy_adds_missing_actions() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create manifest with only one action
    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"
"#;
    let manifest_path = root.join(".github").join("gx.toml");
    let mut manifest_file = fs::File::create(&manifest_path).unwrap();
    manifest_file
        .write_all(manifest_content.as_bytes())
        .unwrap();

    // Create workflow with additional actions
    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
      - uses: docker/build-push-action@v5
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command
    let result = run_tidy(&root);
    assert!(result.is_ok());

    // Verify missing actions were added
    let updated_manifest = fs::read_to_string(&manifest_path).unwrap();
    assert!(updated_manifest.contains("actions/checkout"));
    assert!(updated_manifest.contains("actions/setup-node"));
    assert!(updated_manifest.contains("docker/build-push-action"));
}

#[test]
fn test_gx_tidy_preserves_existing_versions() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create manifest with specific version
    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"
"#;
    let manifest_path = root.join(".github").join("gx.toml");
    let mut manifest_file = fs::File::create(&manifest_path).unwrap();
    manifest_file
        .write_all(manifest_content.as_bytes())
        .unwrap();

    // Create workflow using older version
    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v3
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command
    let result = run_tidy(&root);
    assert!(result.is_ok());

    // Verify manifest keeps v4 (manifest is source of truth for versions)
    let updated_manifest = fs::read_to_string(&manifest_path).unwrap();
    assert!(updated_manifest.contains("\"actions/checkout\" = \"v4\""));

    // Verify workflow was updated to v4 with SHA (manifest dictates versions)
    let updated_workflow = fs::read_to_string(&workflow_path).unwrap();
    let checkout_sha = MockRegistry::fake_sha("actions/checkout", "v4");
    assert!(
        updated_workflow.contains(&format!("actions/checkout@{checkout_sha} # v4")),
        "Expected checkout with SHA, got:\n{updated_workflow}"
    );
}

#[test]
fn test_gx_tidy_multiple_workflows() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create empty manifest to trigger file-backed mode
    create_empty_manifest(&root);

    // Create first workflow
    let ci_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
";
    let ci_path = root.join(".github").join("workflows").join("ci.yml");
    let mut ci_file = fs::File::create(&ci_path).unwrap();
    ci_file.write_all(ci_content.as_bytes()).unwrap();

    // Create second workflow
    let deploy_content = "name: Deploy
jobs:
  deploy:
    steps:
      - uses: docker/build-push-action@v5
";
    let deploy_path = root.join(".github").join("workflows").join("deploy.yml");
    let mut deploy_file = fs::File::create(&deploy_path).unwrap();
    deploy_file.write_all(deploy_content.as_bytes()).unwrap();

    // Execute command
    let result = run_tidy(&root);
    assert!(result.is_ok());

    // Verify manifest contains actions from both workflows
    let manifest_path = root.join(".github").join("gx.toml");
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_content.contains("actions/checkout"));
    assert!(manifest_content.contains("docker/build-push-action"));
}

#[test]
fn test_gx_tidy_skips_local_actions() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create empty manifest to trigger file-backed mode
    create_empty_manifest(&root);

    // Create workflow with local action
    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - uses: ./local/action
      - uses: ./.github/actions/my-action
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command
    let result = run_tidy(&root);
    assert!(result.is_ok());

    // Verify manifest only contains remote action
    let manifest_path = root.join(".github").join("gx.toml");
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_content.contains("actions/checkout"));
    assert!(!manifest_content.contains("./local"));
    assert!(!manifest_content.contains(".github/actions"));
}

#[test]
fn test_gx_tidy_no_workflows() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // No workflows, just empty directory

    // Execute command - should succeed
    let result = run_tidy(&root);
    assert!(result.is_ok());
}

#[test]
fn test_gx_tidy_workflow_without_actions() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create workflow without uses statements
    let workflow_content = r#"name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo "Hello"
"#;
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command - should succeed
    let result = run_tidy(&root);
    assert!(result.is_ok());
}

#[test]
fn test_gx_tidy_multiple_versions_picks_highest() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create empty manifest to trigger file-backed mode
    create_empty_manifest(&root);

    // Create workflow with different versions in different jobs
    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
  test:
    steps:
      - uses: actions/checkout@v3
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command
    let result = run_tidy(&root);
    assert!(result.is_ok());

    // Verify manifest has single global version (highest semver = v4)
    let manifest_path = root.join(".github").join("gx.toml");
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();

    assert!(manifest_content.contains("[actions]"));
    assert!(manifest_content.contains("\"actions/checkout\" = \"v4\""));

    // Should NOT have any workflow overrides
    assert!(!manifest_content.contains("[workflows"));
}

#[test]
fn test_gx_tidy_multiple_workflows_unified_version() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create empty manifest to trigger file-backed mode
    create_empty_manifest(&root);

    // Create two workflows with different versions
    let ci_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
";
    let ci_path = root.join(".github").join("workflows").join("ci.yml");
    let mut ci_file = fs::File::create(&ci_path).unwrap();
    ci_file.write_all(ci_content.as_bytes()).unwrap();

    let deploy_content = "name: Deploy
jobs:
  deploy:
    steps:
      - uses: actions/checkout@v3
";
    let deploy_path = root.join(".github").join("workflows").join("deploy.yml");
    let mut deploy_file = fs::File::create(&deploy_path).unwrap();
    deploy_file.write_all(deploy_content.as_bytes()).unwrap();

    // Execute command
    let result = run_tidy(&root);
    assert!(result.is_ok());

    // Verify manifest has single global version (highest = v4)
    let manifest_path = root.join(".github").join("gx.toml");
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();

    assert!(manifest_content.contains("\"actions/checkout\" = \"v4\""));

    // Should NOT have any workflow overrides
    assert!(!manifest_content.contains("[workflows"));
}

#[test]
fn test_gx_tidy_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create empty manifest to trigger file-backed mode
    create_empty_manifest(&root);

    // Create workflow
    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command twice
    let result1 = run_tidy(&root);
    assert!(result1.is_ok());

    let manifest_after_first = fs::read_to_string(root.join(".github").join("gx.toml")).unwrap();
    let workflow_after_first = fs::read_to_string(&workflow_path).unwrap();

    let result2 = run_tidy(&root);
    assert!(result2.is_ok());

    let manifest_after_second = fs::read_to_string(root.join(".github").join("gx.toml")).unwrap();
    let workflow_after_second = fs::read_to_string(&workflow_path).unwrap();

    // Results should be identical
    assert_eq!(manifest_after_first, manifest_after_second);
    assert_eq!(workflow_after_first, workflow_after_second);
}

#[test]
fn test_gx_tidy_with_sha_and_comment() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create empty manifest to trigger file-backed mode
    create_empty_manifest(&root);

    // Create workflow with SHA and comment tag
    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123def456 # v4
      - uses: actions/setup-node@xyz789 #v3
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command
    let result = run_tidy(&root);
    assert!(result.is_ok());

    // Verify manifest contains version tags from comments, not SHAs
    let manifest_path = root.join(".github").join("gx.toml");
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();

    assert!(manifest_content.contains("\"actions/checkout\" = \"v4\""));
    assert!(manifest_content.contains("\"actions/setup-node\" = \"v3\""));

    // Should NOT contain the SHAs in manifest
    assert!(!manifest_content.contains("abc123def456"));
    assert!(!manifest_content.contains("xyz789"));

    // Note: Lock file is only created when there are SHAs to store.
    // Without GITHUB_TOKEN, no SHAs are resolved, so lock file may not exist.
}

#[test]
fn test_gx_tidy_real_world_workflow_format() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create empty manifest to trigger file-backed mode
    create_empty_manifest(&root);

    // Create workflow with real-world format (name, SHA, and version comment)
    let workflow_content = "on:
  pull_request:

permissions:
  contents: read

jobs:
  test_windows:
    runs-on: windows-2025
    steps:
      - name: Checkout repository
        uses: actions/checkout@8e8c483db84b4bee98b60c0593521ed34d9990e8 # v6.0.1

      - name: Login to Docker Hub
        uses: docker/login-action@5e57cd118135c172c3672efd75eb46360885c0ef # v3.6.0
";
    let workflow_path = root.join(".github").join("workflows").join("windows.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command with a mock registry that knows which tags map to these SHAs
    let manifest_path = root.join(".github").join("gx.toml");
    let lock_path = root.join(".github").join("gx.lock");
    let manifest = parse_manifest(&manifest_path).unwrap();
    let lock = parse_lock(&lock_path).unwrap();
    let registry = MockRegistry::new()
        .with_tags(
            "actions/checkout",
            "8e8c483db84b4bee98b60c0593521ed34d9990e8",
            &["v6", "v6.0.1"],
        )
        .with_tags(
            "docker/login-action",
            "5e57cd118135c172c3672efd75eb46360885c0ef",
            &["v3", "v3.6.0"],
        );
    let scanner = FileWorkflowScanner::new(&root);
    let updater = FileWorkflowUpdater::new(&root);
    let (updated_manifest, updated_lock) =
        tidy::run(manifest, lock, &manifest_path, registry, &scanner, &updater).unwrap();

    // Save the results
    FileManifest::new(&manifest_path)
        .save(&updated_manifest)
        .unwrap();
    FileLock::new(&lock_path).save(&updated_lock).unwrap();

    // Verify manifest contains version tags from comments, not SHAs
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();

    assert!(
        manifest_content.contains("\"actions/checkout\" = \"v6.0.1\""),
        "Expected v6.0.1 in manifest, got: {manifest_content}"
    );
    assert!(
        manifest_content.contains("\"docker/login-action\" = \"v3.6.0\""),
        "Expected v3.6.0 in manifest, got: {manifest_content}"
    );

    // Should NOT contain the SHAs in manifest
    assert!(!manifest_content.contains("8e8c483db84b4bee98b60c0593521ed34d9990e8"));
    assert!(!manifest_content.contains("5e57cd118135c172c3672efd75eb46360885c0ef"));
}

#[test]
fn test_gx_tidy_tag_not_resolved_without_token() {
    // Demonstrates the original bug: without a working registry (no GITHUB_TOKEN),
    // tidy silently leaves tags unchanged instead of reporting the failure.
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_empty_manifest(&root);

    let workflow_content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Run tidy with NoopRegistry (simulates missing GITHUB_TOKEN)
    let manifest_path = root.join(".github").join("gx.toml");
    let lock_path = root.join(".github").join("gx.lock");
    let manifest = parse_manifest(&manifest_path).unwrap();
    let lock = parse_lock(&lock_path).unwrap();
    let scanner = FileWorkflowScanner::new(&root);
    let updater = FileWorkflowUpdater::new(&root);
    let result = tidy::run(
        manifest,
        lock,
        &manifest_path,
        NoopRegistry,
        &scanner,
        &updater,
    );

    // The command should fail when it cannot resolve actions
    assert!(
        result.is_err(),
        "tidy should return an error when actions cannot be resolved"
    );
}

#[test]
fn test_gx_tidy_resolves_tag_to_sha() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create empty manifest to trigger file-backed mode
    create_empty_manifest(&root);

    // Create workflow with tag-only references (no SHA, no comment)
    let workflow_content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Run tidy with a mock registry that resolves versions to SHAs
    let manifest_path = root.join(".github").join("gx.toml");
    let lock_path = root.join(".github").join("gx.lock");
    let manifest = parse_manifest(&manifest_path).unwrap();
    let lock = parse_lock(&lock_path).unwrap();
    let scanner = FileWorkflowScanner::new(&root);
    let updater = FileWorkflowUpdater::new(&root);
    let (updated_manifest, updated_lock) = tidy::run(
        manifest,
        lock,
        &manifest_path,
        MockRegistry::new(),
        &scanner,
        &updater,
    )
    .unwrap();

    // Save the results
    FileManifest::new(&manifest_path)
        .save(&updated_manifest)
        .unwrap();
    FileLock::new(&lock_path).save(&updated_lock).unwrap();

    // Verify the workflow was updated: tags should be replaced with SHAs + version comments
    let updated_workflow = fs::read_to_string(&workflow_path).unwrap();

    let checkout_sha = MockRegistry::fake_sha("actions/checkout", "v4");
    let expected_checkout = format!("actions/checkout@{checkout_sha} # v4");
    assert!(
        updated_workflow.contains(&expected_checkout),
        "Expected workflow to contain '{expected_checkout}', got:\n{updated_workflow}"
    );

    let toolchain_sha = MockRegistry::fake_sha("dtolnay/rust-toolchain", "stable");
    let expected_toolchain = format!("dtolnay/rust-toolchain@{toolchain_sha} # stable");
    assert!(
        updated_workflow.contains(&expected_toolchain),
        "Expected workflow to contain '{expected_toolchain}', got:\n{updated_workflow}"
    );

    // Verify lock file was created with the SHAs
    let lock_content = fs::read_to_string(&lock_path).unwrap();
    assert!(
        lock_content.contains(&checkout_sha),
        "Expected lock to contain checkout SHA, got:\n{lock_content}"
    );
}

#[test]
fn test_gx_tidy_respects_override_for_specific_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Manifest: checkout = v4 globally, but deploy.yml should stay on v3
    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "v3" },
]
"#;
    let manifest_path = root.join(".github").join("gx.toml");
    fs::write(&manifest_path, manifest_content).unwrap();

    // Two workflows
    let ci_content = "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n";
    let deploy_content =
        "name: Deploy\njobs:\n  deploy:\n    steps:\n      - uses: actions/checkout@v3\n";
    fs::write(root.join(".github/workflows/ci.yml"), ci_content).unwrap();
    fs::write(root.join(".github/workflows/deploy.yml"), deploy_content).unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok(), "tidy failed: {:?}", result.err());

    // ci.yml should be updated to v4 SHA
    let ci_updated = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let checkout_v4_sha = MockRegistry::fake_sha("actions/checkout", "v4");
    assert!(
        ci_updated.contains(&format!("actions/checkout@{checkout_v4_sha} # v4")),
        "ci.yml should use v4 SHA, got:\n{ci_updated}"
    );

    // deploy.yml should use v3 SHA (override applies)
    let deploy_updated = fs::read_to_string(root.join(".github/workflows/deploy.yml")).unwrap();
    let checkout_v3_sha = MockRegistry::fake_sha("actions/checkout", "v3");
    assert!(
        deploy_updated.contains(&format!("actions/checkout@{checkout_v3_sha} # v3")),
        "deploy.yml should use v3 SHA from override, got:\n{deploy_updated}"
    );
}

#[test]
fn test_gx_tidy_override_job_level() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/ci.yml", job = "legacy-build", version = "v3" },
]
"#;
    fs::write(root.join(".github/gx.toml"), manifest_content).unwrap();

    let ci_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
  legacy-build:
    steps:
      - uses: actions/checkout@v3
";
    fs::write(root.join(".github/workflows/ci.yml"), ci_content).unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok(), "{:?}", result.err());

    // The lock should have both versions since both are in manifest (global + override)
    let lock_content = fs::read_to_string(root.join(".github/gx.lock")).unwrap();
    let v4_sha = MockRegistry::fake_sha("actions/checkout", "v4");
    let v3_sha = MockRegistry::fake_sha("actions/checkout", "v3");
    assert!(lock_content.contains(&v4_sha), "Lock should have v4 SHA");
    assert!(lock_content.contains(&v3_sha), "Lock should have v3 SHA");
}

#[test]
fn test_gx_tidy_removes_stale_override() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Override references a workflow that no longer exists
    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/old-workflow.yml", version = "v3" },
]
"#;
    fs::write(root.join(".github/gx.toml"), manifest_content).unwrap();
    fs::write(
        root.join(".github/workflows/ci.yml"),
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
    )
    .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok(), "{:?}", result.err());

    let manifest_content = fs::read_to_string(root.join(".github/gx.toml")).unwrap();
    assert!(
        !manifest_content.contains("old-workflow.yml"),
        "Stale override should be removed, got:\n{manifest_content}"
    );
}

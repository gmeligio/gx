use gx::commands::tidy;
use gx::infrastructure::{FileLock, FileManifest, MemoryLock, MemoryManifest};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

fn create_test_repo(temp_dir: &TempDir) -> std::path::PathBuf {
    let root = temp_dir.path();
    let github_dir = root.join(".github");
    let workflows_dir = github_dir.join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();
    root.to_path_buf()
}

/// Helper to run tidy with appropriate DI based on manifest existence (same logic as main.rs)
fn run_tidy(repo_root: &Path) -> anyhow::Result<()> {
    let manifest_path = repo_root.join(".github").join("gx.toml");
    let lock_path = repo_root.join(".github").join("gx.lock");

    if manifest_path.exists() {
        let manifest = FileManifest::load_or_default(&manifest_path)?;
        let lock = FileLock::load_or_default(&lock_path)?;
        tidy::run(repo_root, manifest, lock)
    } else {
        let manifest = MemoryManifest::default();
        let lock = MemoryLock::default();
        tidy::run(repo_root, manifest, lock)
    }
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

    // Workflow should remain unchanged in memory-only mode (no SHAs resolved)
    let workflow_content_after = fs::read_to_string(&workflow_path).unwrap();
    assert!(workflow_content_after.contains("actions/checkout@v4"));
    assert!(workflow_content_after.contains("actions/setup-node@v3"));
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

    // Verify workflow was updated
    let updated_content = fs::read_to_string(&workflow_path).unwrap();
    assert!(updated_content.contains("actions/checkout@v4"));
    assert!(updated_content.contains("actions/setup-node@v4"));
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

    // Verify workflow was updated to v4 (manifest dictates versions)
    let updated_workflow = fs::read_to_string(&workflow_path).unwrap();
    assert!(updated_workflow.contains("actions/checkout@v4"));
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

    // Execute command
    let result = run_tidy(&root);
    assert!(result.is_ok());

    // Verify manifest contains version tags from comments, not SHAs
    let manifest_path = root.join(".github").join("gx.toml");
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();

    println!("=== Manifest content ===");
    println!("{manifest_content}");

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

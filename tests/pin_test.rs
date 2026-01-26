use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn create_test_repo(temp_dir: &TempDir) -> std::path::PathBuf {
    let root = temp_dir.path();
    let github_dir = root.join(".github");
    let workflows_dir = github_dir.join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();
    root.to_path_buf()
}

#[test]
fn test_gx_pin_updates_workflows() {
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

    // Create workflow
    let workflow_content = r#"name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
"#;
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    // Execute command
    let result = gx::commands::pin::run(&root);
    assert!(result.is_ok());

    // Verify workflow was updated
    let updated_content = fs::read_to_string(&workflow_path).unwrap();
    assert!(updated_content.contains("actions/checkout@v4"));
    assert!(updated_content.contains("actions/setup-node@v4"));
    assert!(!updated_content.contains("@v3"));
}

#[test]
fn test_gx_pin_with_no_workflows() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    // Create manifest only, no workflows
    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"
"#;
    let manifest_path = root.join(".github").join("gx.toml");
    let mut manifest_file = fs::File::create(&manifest_path).unwrap();
    manifest_file
        .write_all(manifest_content.as_bytes())
        .unwrap();

    // Execute command - should succeed but not update anything
    let result = gx::commands::pin::run(&root);
    assert!(result.is_ok());
}

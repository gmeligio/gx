use anyhow::{Context, Result};
use glob::glob;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::git::is_commit_sha;

pub struct WorkflowUpdater {
    workflows_dir: PathBuf,
}

pub struct UpdateResult {
    pub file: PathBuf,
    pub changes: Vec<String>,
}

/// Location of an action within a workflow file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionLocation {
    pub workflow: String,
    pub job: String,
    pub step_index: usize,
}

/// An action extracted from a workflow file with full location info
#[derive(Debug, Clone)]
pub struct ExtractedAction {
    pub name: String,
    pub version: String,
    /// The commit SHA if present in the workflow (when format is `SHA # version`)
    pub sha: Option<String>,
    pub file: PathBuf,
    pub location: ActionLocation,
}

/// Minimal workflow structure for YAML parsing
#[derive(Debug, Deserialize)]
struct Workflow {
    #[serde(default)]
    jobs: HashMap<String, Job>,
}

#[derive(Debug, Deserialize)]
struct Job {
    #[serde(default)]
    steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
struct Step {
    uses: Option<String>,
}

impl WorkflowUpdater {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            workflows_dir: repo_root.join(".github").join("workflows"),
        }
    }

    pub fn find_workflows(&self) -> Result<Vec<PathBuf>> {
        let mut workflows = Vec::new();

        for extension in &["yml", "yaml"] {
            let pattern = self
                .workflows_dir
                .join(format!("*.{}", extension))
                .to_string_lossy()
                .to_string();

            for entry in glob(&pattern).context("Failed to read glob pattern")? {
                match entry {
                    Ok(path) => workflows.push(path),
                    Err(e) => eprintln!("Warning: Error reading path: {}", e),
                }
            }
        }

        Ok(workflows)
    }

    pub fn update_workflow(
        &self,
        workflow_path: &Path,
        actions: &HashMap<String, String>,
    ) -> Result<UpdateResult> {
        let content = fs::read_to_string(workflow_path)
            .with_context(|| format!("Failed to read workflow: {}", workflow_path.display()))?;

        let mut updated_content = content.clone();
        let mut changes = Vec::new();

        for (action, version) in actions {
            let escaped_action = regex::escape(action);
            // Match "uses: action@ref" and optionally capture any existing comment
            let pattern = format!(r"(uses:\s*{})@[^\s#]+(\s*#[^\n]*)?", escaped_action);
            let re = Regex::new(&pattern)?;

            if re.is_match(&updated_content) {
                let replacement = format!("${{1}}@{}", version);
                let new_content = re.replace_all(&updated_content, replacement.as_str());

                if new_content != updated_content {
                    changes.push(format!("{}@{}", action, version));
                    updated_content = new_content.to_string();
                }
            }
        }

        if !changes.is_empty() {
            fs::write(workflow_path, &updated_content).with_context(|| {
                format!("Failed to write workflow: {}", workflow_path.display())
            })?;
        }

        Ok(UpdateResult {
            file: workflow_path.to_path_buf(),
            changes,
        })
    }

    pub fn update_all(&self, actions: &HashMap<String, String>) -> Result<Vec<UpdateResult>> {
        let workflows = self.find_workflows()?;
        let mut results = Vec::new();

        for workflow in workflows {
            let result = self.update_workflow(&workflow, actions)?;
            if !result.changes.is_empty() {
                results.push(result);
            }
        }

        Ok(results)
    }

    pub fn extract_actions(&self, workflow_path: &Path) -> Result<Vec<ExtractedAction>> {
        let content = fs::read_to_string(workflow_path)
            .with_context(|| format!("Failed to read workflow: {}", workflow_path.display()))?;

        let workflow_name = workflow_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut actions = Vec::new();

        // First, build a map of uses line -> comment version from raw content
        let mut version_comments = HashMap::new();
        let uses_with_comment_re = Regex::new(r"uses:\s*([^#\n]+)#\s*v?(\S+)")?;

        for line in content.lines() {
            if let Some(cap) = uses_with_comment_re.captures(line) {
                let uses_part = cap[1].trim().to_string();
                let mut comment_version = cap[2].to_string();
                // Normalize: ensure it starts with 'v'
                if !comment_version.starts_with('v') {
                    comment_version = format!("v{}", comment_version);
                }
                version_comments.insert(uses_part, comment_version);
            }
        }

        // Parse YAML to get structured job/step info
        let workflow: Workflow = serde_yaml_ng::from_str(&content).with_context(|| {
            format!("Failed to parse workflow YAML: {}", workflow_path.display())
        })?;

        // Pattern to parse uses: owner/repo@ref
        let uses_re = Regex::new(r"^([^@\s]+)@([^\s#]+)")?;

        for (job_name, job) in &workflow.jobs {
            for (step_index, step) in job.steps.iter().enumerate() {
                if let Some(uses) = &step.uses {
                    if let Some(cap) = uses_re.captures(uses) {
                        let name = cap[1].to_string();
                        let ref_or_sha = cap[2].to_string();

                        // Skip local actions (./path) and docker actions (docker://)
                        if name.starts_with('.') || name.starts_with("docker://") {
                            continue;
                        }

                        // Extract version and SHA from uses line
                        // If there's a comment, use comment as version; if ref is SHA, store it
                        let (version, sha) =
                            if let Some(comment_version) = version_comments.get(uses) {
                                // Has a comment - use comment as version
                                // If ref is a SHA, store it
                                let sha = if is_commit_sha(&ref_or_sha) {
                                    Some(ref_or_sha)
                                } else {
                                    None
                                };
                                (comment_version.clone(), sha)
                            } else {
                                // No comment, use the ref as-is, no SHA stored
                                (ref_or_sha, None)
                            };

                        actions.push(ExtractedAction {
                            name,
                            version,
                            sha,
                            file: workflow_path.to_path_buf(),
                            location: ActionLocation {
                                workflow: workflow_name.clone(),
                                job: job_name.clone(),
                                step_index,
                            },
                        });
                    }
                }
            }
        }

        Ok(actions)
    }

    pub fn extract_all(&self) -> Result<Vec<ExtractedAction>> {
        let workflows = self.find_workflows()?;
        let mut all_actions = Vec::new();

        for workflow in workflows {
            let actions = self.extract_actions(&workflow)?;
            all_actions.extend(actions);
        }

        Ok(all_actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_workflow(dir: &Path, name: &str, content: &str) -> PathBuf {
        let workflows_dir = dir.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();
        let file_path = workflows_dir.join(name);
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file_path
    }

    #[test]
    fn test_find_workflows() {
        let temp_dir = TempDir::new().unwrap();
        create_test_workflow(temp_dir.path(), "ci.yml", "name: CI");
        create_test_workflow(temp_dir.path(), "deploy.yaml", "name: Deploy");

        let updater = WorkflowUpdater::new(temp_dir.path());
        let workflows = updater.find_workflows().unwrap();

        assert_eq!(workflows.len(), 2);
    }

    #[test]
    fn test_update_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let mut actions = HashMap::new();
        actions.insert("actions/checkout".to_string(), "v4".to_string());

        let result = updater.update_workflow(&workflow_path, &actions).unwrap();

        assert_eq!(result.changes.len(), 1);
        assert!(result.changes[0].contains("actions/checkout@v4"));

        let updated = fs::read_to_string(&workflow_path).unwrap();
        assert!(updated.contains("actions/checkout@v4"));
        assert!(updated.contains("actions/setup-node@v3")); // unchanged
    }

    #[test]
    fn test_extract_actions() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
      - uses: docker/build-push-action@v5
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let actions = updater.extract_actions(&workflow_path).unwrap();

        assert_eq!(actions.len(), 3);

        // Check that all actions were found (order may vary due to HashMap)
        let action_names: Vec<_> = actions.iter().map(|a| a.name.as_str()).collect();
        assert!(action_names.contains(&"actions/checkout"));
        assert!(action_names.contains(&"actions/setup-node"));
        assert!(action_names.contains(&"docker/build-push-action"));

        // Check location info
        for action in &actions {
            assert_eq!(action.location.workflow, "ci.yml");
            assert_eq!(action.location.job, "build");
        }
    }

    #[test]
    fn test_extract_actions_skips_local() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - uses: ./local/action
      - uses: ./.github/actions/my-action
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let actions = updater.extract_actions(&workflow_path).unwrap();

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "actions/checkout");
    }

    #[test]
    fn test_extract_actions_multiple_jobs() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
  test:
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let actions = updater.extract_actions(&workflow_path).unwrap();

        assert_eq!(actions.len(), 3);

        // Find actions by job
        let build_actions: Vec<_> = actions
            .iter()
            .filter(|a| a.location.job == "build")
            .collect();
        let test_actions: Vec<_> = actions
            .iter()
            .filter(|a| a.location.job == "test")
            .collect();

        assert_eq!(build_actions.len(), 1);
        assert_eq!(test_actions.len(), 2);
    }

    #[test]
    fn test_extract_all() {
        let temp_dir = TempDir::new().unwrap();
        create_test_workflow(
            temp_dir.path(),
            "ci.yml",
            "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4",
        );
        create_test_workflow(
            temp_dir.path(),
            "deploy.yml",
            "jobs:\n  deploy:\n    steps:\n      - uses: docker/build-push-action@v5",
        );

        let updater = WorkflowUpdater::new(temp_dir.path());
        let actions = updater.extract_all().unwrap();

        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn test_extract_actions_with_sha_and_comment() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123def456 # v4
      - uses: actions/setup-node@xyz789 #v3
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let actions = updater.extract_actions(&workflow_path).unwrap();

        assert_eq!(actions.len(), 2);

        let checkout = actions
            .iter()
            .find(|a| a.name == "actions/checkout")
            .unwrap();
        assert_eq!(checkout.version, "v4");

        let setup_node = actions
            .iter()
            .find(|a| a.name == "actions/setup-node")
            .unwrap();
        assert_eq!(setup_node.version, "v3");
    }

    #[test]
    fn test_extract_actions_comment_without_v_prefix() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123 # 4
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let actions = updater.extract_actions(&workflow_path).unwrap();

        assert_eq!(actions.len(), 1);
        // Should normalize to v4
        assert_eq!(actions[0].version, "v4");
    }

    #[test]
    fn test_extract_actions_tag_without_comment() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let actions = updater.extract_actions(&workflow_path).unwrap();

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].version, "v4");
    }

    #[test]
    fn test_extract_actions_sha_without_comment() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123def456
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let actions = updater.extract_actions(&workflow_path).unwrap();

        assert_eq!(actions.len(), 1);
        // Should use SHA as version when no comment
        assert_eq!(actions[0].version, "abc123def456");
    }

    #[test]
    fn test_extract_actions_real_world_format() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"on:
  pull_request:

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@8e8c483db84b4bee98b60c0593521ed34d9990e8 # v6.0.1

      - name: Login
        uses: docker/login-action@5e57cd118135c172c3672efd75eb46360885c0ef # v3.6.0
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "test.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let actions = updater.extract_actions(&workflow_path).unwrap();

        assert_eq!(actions.len(), 2);

        let checkout = actions
            .iter()
            .find(|a| a.name == "actions/checkout")
            .unwrap();
        assert_eq!(checkout.version, "v6.0.1");
        assert_eq!(
            checkout.sha.as_deref(),
            Some("8e8c483db84b4bee98b60c0593521ed34d9990e8")
        );

        let login = actions
            .iter()
            .find(|a| a.name == "docker/login-action")
            .unwrap();
        assert_eq!(login.version, "v3.6.0");
        assert_eq!(
            login.sha.as_deref(),
            Some("5e57cd118135c172c3672efd75eb46360885c0ef")
        );
    }

    #[test]
    fn test_extract_actions_sha_field_none_for_tag() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let actions = updater.extract_actions(&workflow_path).unwrap();

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].version, "v4");
        // No SHA when using a tag without comment
        assert!(actions[0].sha.is_none());
    }

    #[test]
    fn test_extract_actions_sha_field_none_for_short_ref() {
        let temp_dir = TempDir::new().unwrap();
        // Short SHA (not 40 chars) with comment
        let content = r#"name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123 # v4
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let actions = updater.extract_actions(&workflow_path).unwrap();

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].version, "v4");
        // Short refs are not treated as SHAs
        assert!(actions[0].sha.is_none());
    }

    #[test]
    fn test_update_workflow_uses_commit_sha() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let mut actions = HashMap::new();
        // Simulate the format from lock.build_update_map(): "SHA # version"
        actions.insert(
            "actions/checkout".to_string(),
            "abc123def456 # v4".to_string(),
        );

        let result = updater.update_workflow(&workflow_path, &actions).unwrap();

        assert_eq!(result.changes.len(), 1);

        // Verify the workflow was updated with the SHA, not the tag
        let updated = fs::read_to_string(&workflow_path).unwrap();
        assert!(
            updated.contains("actions/checkout@abc123def456 # v4"),
            "Expected SHA with comment, got: {}",
            updated
        );
        assert!(
            !updated.contains("actions/checkout@v4"),
            "Should not contain tag without SHA"
        );
    }

    #[test]
    fn test_update_workflow_no_duplicate_comments() {
        let temp_dir = TempDir::new().unwrap();
        // Start with a workflow that already has a comment
        let content = r#"name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3 # v3
      - uses: actions/setup-node@old_sha # v2
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let mut actions = HashMap::new();
        // Update both actions with new SHAs
        actions.insert(
            "actions/checkout".to_string(),
            "abc123def456 # v4".to_string(),
        );
        actions.insert(
            "actions/setup-node".to_string(),
            "xyz789012345 # v3".to_string(),
        );

        let result = updater.update_workflow(&workflow_path, &actions).unwrap();

        assert_eq!(result.changes.len(), 2);

        // Verify no duplicate comments
        let updated = fs::read_to_string(&workflow_path).unwrap();

        // Should have the new SHA with new comment
        assert!(
            updated.contains("actions/checkout@abc123def456 # v4"),
            "Expected new SHA with comment, got: {}",
            updated
        );

        // Should NOT have duplicate comments like "# v4 # v3"
        assert!(
            !updated.contains("# v4 # v3"),
            "Found duplicate comment in: {}",
            updated
        );
        assert!(
            !updated.contains("# v3 # v3"),
            "Found duplicate comment in: {}",
            updated
        );

        // Verify setup-node was also updated correctly
        assert!(
            updated.contains("actions/setup-node@xyz789012345 # v3"),
            "Expected setup-node with new SHA and comment, got: {}",
            updated
        );
        assert!(
            !updated.contains("# v3 # v2"),
            "Found duplicate comment in: {}",
            updated
        );
    }
}

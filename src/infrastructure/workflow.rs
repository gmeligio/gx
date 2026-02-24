use glob::glob;
use log::{debug, info, warn};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::{ActionId, UpdateResult, UsesRef, WorkflowError};

/// Action data extracted from a workflow file.
/// Call `uses_ref.interpret()` to get domain types.
#[derive(Debug, Clone)]
struct ExtractedAction {
    uses_ref: UsesRef,
    location: crate::domain::WorkflowLocation,
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

/// Parser for extracting action information from workflow files
pub struct FileWorkflowScanner {
    repo_root: PathBuf,
    workflows_dir: PathBuf,
}

impl FileWorkflowScanner {
    #[must_use]
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            workflows_dir: repo_root.join(".github").join("workflows"),
        }
    }

    /// Compute the path relative to the repo root for use in `WorkflowLocation`.
    fn rel_path(&self, workflow_path: &Path) -> String {
        workflow_path
            .strip_prefix(&self.repo_root)
            .unwrap_or(workflow_path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// Find all workflow files in the repository's `.github/workflows` folder.
    ///
    /// # Errors
    ///
    /// Returns an error if the glob pattern is invalid.
    pub fn find_workflows(&self) -> Result<Vec<PathBuf>, WorkflowError> {
        let mut workflows = Vec::new();

        for extension in &["yml", "yaml"] {
            let pattern = self
                .workflows_dir
                .join(format!("*.{extension}"))
                .to_string_lossy()
                .to_string();

            for entry in glob(&pattern)? {
                match entry {
                    Ok(path) => workflows.push(path),
                    Err(e) => warn!("Error reading path: {e}"),
                }
            }
        }

        Ok(workflows)
    }

    /// Extract all actions from a single workflow file as data.
    ///
    /// Returns extraction without any interpretation.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, parsed as YAML, or the regex pattern is invalid.
    fn extract_actions(
        workflow_path: &Path,
        workflow_rel_path: &str,
    ) -> Result<Vec<ExtractedAction>, WorkflowError> {
        let content = fs::read_to_string(workflow_path).map_err(|source| WorkflowError::Read {
            path: workflow_path.to_path_buf(),
            source,
        })?;

        let mut actions = Vec::new();

        // Build a map of uses line -> comment text from content
        // Note: We capture the comment as-is without normalization
        let mut comments = HashMap::new();
        let uses_with_comment_re = Regex::new(r"uses:\s*([^#\n]+)#\s*(\S+)")?;

        for line in content.lines() {
            if let Some(cap) = uses_with_comment_re.captures(line) {
                let uses_part = cap[1].trim().to_string();
                let comment = cap[2].to_string();
                comments.insert(uses_part, comment);
            }
        }

        // Parse YAML to get structured job/step info
        let workflow: Workflow =
            serde_saphyr::from_str(&content).map_err(|source| WorkflowError::Parse {
                path: workflow_path.to_path_buf(),
                source: Box::new(source),
            })?;

        // Pattern to parse uses: owner/repo@ref
        let uses_re = Regex::new(r"^([^@\s]+)@([^\s#]+)")?;

        for (job_id, job) in &workflow.jobs {
            for (step_idx, step) in job.steps.iter().enumerate() {
                if let Some(uses) = &step.uses
                    && let Some(cap) = uses_re.captures(uses)
                {
                    let action_name = cap[1].to_string();
                    let uses_ref = cap[2].to_string();

                    // Skip local actions (./path) and docker actions (docker://)
                    if action_name.starts_with('.') || action_name.starts_with("docker://") {
                        continue;
                    }

                    // Get comment if present (raw, no normalization)
                    let comment = comments.get(uses).cloned();

                    actions.push(ExtractedAction {
                        uses_ref: UsesRef::new(action_name, uses_ref, comment),
                        location: crate::domain::WorkflowLocation {
                            workflow: workflow_rel_path.to_string(),
                            job: Some(job_id.clone()),
                            step: Some(step_idx),
                        },
                    });
                }
            }
        }

        Ok(actions)
    }

    /// Scan a single workflow and aggregate actions into a `WorkflowActionSet`.
    ///
    /// # Errors
    ///
    /// Returns an error if the workflow file cannot be processed.
    pub fn scan(
        &self,
        workflow_path: &Path,
    ) -> Result<crate::domain::WorkflowActionSet, WorkflowError> {
        let rel = self.rel_path(workflow_path);
        let actions = Self::extract_actions(workflow_path, &rel)?;
        let mut action_set = crate::domain::WorkflowActionSet::new();
        for action in &actions {
            action_set.add(&action.uses_ref.interpret());
        }
        Ok(action_set)
    }

    /// Scan all workflows and aggregate actions into a `WorkflowActionSet`.
    ///
    /// # Errors
    ///
    /// Returns an error if any workflow file cannot be processed.
    pub fn scan_all(&self) -> Result<crate::domain::WorkflowActionSet, WorkflowError> {
        let workflows = self.find_workflows()?;
        if workflows.is_empty() {
            info!("No workflows found in .github/workflows/");
            return Ok(crate::domain::WorkflowActionSet::new());
        }

        debug!("Scanning workflows...");
        for workflow in &workflows {
            debug!("{}", workflow.display());
        }

        let mut action_set = crate::domain::WorkflowActionSet::new();
        for workflow in &workflows {
            let rel = self.rel_path(workflow);
            let actions = Self::extract_actions(workflow, &rel)?;
            for action in &actions {
                action_set.add(&action.uses_ref.interpret());
            }
        }
        Ok(action_set)
    }

    /// Scan all workflows and return one `LocatedAction` per step.
    ///
    /// # Errors
    ///
    /// Returns an error if any workflow file cannot be processed.
    pub fn scan_all_located(&self) -> Result<Vec<crate::domain::LocatedAction>, WorkflowError> {
        let workflows = self.find_workflows()?;
        let mut result = Vec::new();
        for workflow in &workflows {
            let rel = self.rel_path(workflow);
            let actions = Self::extract_actions(workflow, &rel)?;
            for action in actions {
                let interpreted = action.uses_ref.interpret();
                result.push(crate::domain::LocatedAction {
                    id: interpreted.id,
                    version: interpreted.version,
                    sha: interpreted.sha,
                    location: action.location,
                });
            }
        }
        Ok(result)
    }
}

/// Writer for updating action versions in workflow files
pub struct FileWorkflowUpdater {
    workflows_dir: PathBuf,
}

impl FileWorkflowUpdater {
    #[must_use]
    pub fn new(repo_root: &Path) -> Self {
        Self {
            workflows_dir: repo_root.join(".github").join("workflows"),
        }
    }

    /// Find all workflow files in the repository's `.github/workflows` folder.
    ///
    /// # Errors
    ///
    /// Returns an error if the glob pattern is invalid.
    pub fn find_workflows(&self) -> Result<Vec<PathBuf>, WorkflowError> {
        let mut workflows = Vec::new();

        for extension in &["yml", "yaml"] {
            let pattern = self
                .workflows_dir
                .join(format!("*.{extension}"))
                .to_string_lossy()
                .to_string();

            for entry in glob(&pattern)? {
                match entry {
                    Ok(path) => workflows.push(path),
                    Err(e) => warn!("Error reading path: {e}"),
                }
            }
        }

        Ok(workflows)
    }

    /// Update action versions in a single workflow file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, the regex pattern is invalid, or the file cannot be written.
    pub fn update_workflow(
        &self,
        workflow_path: &Path,
        actions: &HashMap<ActionId, String>,
    ) -> Result<UpdateResult, WorkflowError> {
        let content = fs::read_to_string(workflow_path).map_err(|source| WorkflowError::Read {
            path: workflow_path.to_path_buf(),
            source,
        })?;

        let mut updated_content = content.clone();
        let mut changes = Vec::new();

        // Compile all regexes upfront before modifying content
        let compiled: Vec<(Regex, String, String)> = actions
            .iter()
            .map(|(action, version)| {
                let escaped = regex::escape(action.as_str());
                let pattern = format!(r"(uses:\s*{escaped})@[^\s#]+(\s*#[^\n]*)?");
                let replacement = format!("${{1}}@{version}");
                let change_label = format!("{action}@{version}");
                Regex::new(&pattern).map(|re| (re, replacement, change_label))
            })
            .collect::<Result<_, _>>()?;

        for (re, replacement, change_label) in &compiled {
            if re.is_match(&updated_content) {
                let new_content = re.replace_all(&updated_content, replacement.as_str());
                if new_content != updated_content {
                    changes.push(change_label.clone());
                    updated_content = new_content.to_string();
                }
            }
        }

        if !changes.is_empty() {
            fs::write(workflow_path, &updated_content).map_err(|source| WorkflowError::Write {
                path: workflow_path.to_path_buf(),
                source,
            })?;
        }

        Ok(UpdateResult {
            file: workflow_path.to_path_buf(),
            changes,
        })
    }

    /// Update action versions in all workflow files.
    ///
    /// # Errors
    ///
    /// Returns an error if any workflow file cannot be processed.
    pub fn update_all(
        &self,
        actions: &HashMap<ActionId, String>,
    ) -> Result<Vec<UpdateResult>, WorkflowError> {
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
}

impl crate::domain::WorkflowScanner for FileWorkflowScanner {
    fn scan_all(&self) -> Result<crate::domain::WorkflowActionSet, WorkflowError> {
        self.scan_all()
    }
}

impl crate::domain::WorkflowUpdater for FileWorkflowUpdater {
    fn update_all(
        &self,
        actions: &HashMap<ActionId, String>,
    ) -> Result<Vec<UpdateResult>, WorkflowError> {
        self.update_all(actions)
    }

    fn update_file(
        &self,
        workflow_path: &Path,
        actions: &HashMap<ActionId, String>,
    ) -> Result<UpdateResult, WorkflowError> {
        self.update_workflow(workflow_path, actions)
    }
}

impl crate::domain::WorkflowScannerLocated for FileWorkflowScanner {
    fn scan_all_located(&self) -> Result<Vec<crate::domain::LocatedAction>, WorkflowError> {
        self.scan_all_located()
    }

    fn find_workflow_paths(&self) -> Result<Vec<PathBuf>, WorkflowError> {
        self.find_workflows()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ActionId, CommitSha};
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
    fn test_scan_all_located_includes_location() {
        let temp_dir = TempDir::new().unwrap();
        let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
  test:
    steps:
      - uses: actions/checkout@v3
";
        create_test_workflow(temp_dir.path(), "ci.yml", content);

        let scanner = FileWorkflowScanner::new(temp_dir.path());
        let located = scanner.scan_all_located().unwrap();

        assert_eq!(located.len(), 3);

        // Find the build-job checkout entry
        let build_checkout = located.iter().find(|a| {
            a.id == ActionId::from("actions/checkout")
                && a.location.job.as_deref() == Some("build")
        });
        assert!(build_checkout.is_some());
        let bc = build_checkout.unwrap();
        assert_eq!(bc.version.as_str(), "v4");
        assert_eq!(bc.location.step, Some(0));

        let test_checkout = located.iter().find(|a| {
            a.id == ActionId::from("actions/checkout")
                && a.location.job.as_deref() == Some("test")
        });
        assert!(test_checkout.is_some());
        assert_eq!(test_checkout.unwrap().version.as_str(), "v3");
    }

    #[test]
    fn test_find_workflows() {
        let temp_dir = TempDir::new().unwrap();
        create_test_workflow(temp_dir.path(), "ci.yml", "name: CI");
        create_test_workflow(temp_dir.path(), "deploy.yaml", "name: Deploy");

        let parser = FileWorkflowScanner::new(temp_dir.path());
        let workflows = parser.find_workflows().unwrap();

        assert_eq!(workflows.len(), 2);
    }

    #[test]
    fn test_update_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
";
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let writer = FileWorkflowUpdater::new(temp_dir.path());
        let mut actions = HashMap::new();
        actions.insert(ActionId::from("actions/checkout"), "v4".to_string());

        let result = writer.update_workflow(&workflow_path, &actions).unwrap();

        assert_eq!(result.changes.len(), 1);
        assert!(result.changes[0].contains("actions/checkout@v4"));

        let updated_workflow = fs::read_to_string(&workflow_path).unwrap();
        assert!(updated_workflow.contains("actions/checkout@v4"));
        assert!(updated_workflow.contains("actions/setup-node@v3")); // unchanged
    }

    #[test]
    fn test_scan_single_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
      - uses: docker/build-push-action@v5
";
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let parser = FileWorkflowScanner::new(temp_dir.path());
        let action_set = parser.scan(&workflow_path).unwrap();

        let ids = action_set.action_ids();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&ActionId::from("actions/checkout")));
        assert!(ids.contains(&ActionId::from("actions/setup-node")));
        assert!(ids.contains(&ActionId::from("docker/build-push-action")));
    }

    #[test]
    fn test_scan_skips_local() {
        let temp_dir = TempDir::new().unwrap();
        let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - uses: ./local/action
      - uses: ./.github/actions/my-action
";
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let parser = FileWorkflowScanner::new(temp_dir.path());
        let action_set = parser.scan(&workflow_path).unwrap();

        let ids = action_set.action_ids();
        assert_eq!(ids.len(), 1);
        assert!(ids.contains(&ActionId::from("actions/checkout")));
    }

    #[test]
    fn test_scan_multiple_jobs() {
        let temp_dir = TempDir::new().unwrap();
        let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
  test:
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
";
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let parser = FileWorkflowScanner::new(temp_dir.path());
        let action_set = parser.scan(&workflow_path).unwrap();

        // Two unique actions (checkout appears in both jobs with different versions)
        assert_eq!(action_set.action_ids().len(), 2);

        let checkout_versions = action_set.versions_for(&ActionId::from("actions/checkout"));
        assert_eq!(checkout_versions.len(), 2);
    }

    #[test]
    fn test_scan_all() {
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

        let parser = FileWorkflowScanner::new(temp_dir.path());
        let action_set = parser.scan_all().unwrap();

        assert_eq!(action_set.action_ids().len(), 2);
    }

    #[test]
    fn test_scan_with_sha_and_comment() {
        let temp_dir = TempDir::new().unwrap();
        let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123def456 # v4
      - uses: actions/setup-node@xyz789 #v3
";
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let parser = FileWorkflowScanner::new(temp_dir.path());
        let action_set = parser.scan(&workflow_path).unwrap();

        let checkout_versions = action_set.versions_for(&ActionId::from("actions/checkout"));
        assert_eq!(checkout_versions[0].as_str(), "v4");

        let node_versions = action_set.versions_for(&ActionId::from("actions/setup-node"));
        assert_eq!(node_versions[0].as_str(), "v3");
    }

    #[test]
    fn test_scan_comment_without_v_prefix() {
        let temp_dir = TempDir::new().unwrap();
        let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123 # 4
";
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let parser = FileWorkflowScanner::new(temp_dir.path());
        let action_set = parser.scan(&workflow_path).unwrap();

        let versions = action_set.versions_for(&ActionId::from("actions/checkout"));
        // Should normalize to v4
        assert_eq!(versions[0].as_str(), "v4");
    }

    #[test]
    fn test_scan_tag_without_comment() {
        let temp_dir = TempDir::new().unwrap();
        let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
";
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let parser = FileWorkflowScanner::new(temp_dir.path());
        let action_set = parser.scan(&workflow_path).unwrap();

        let versions = action_set.versions_for(&ActionId::from("actions/checkout"));
        assert_eq!(versions[0].as_str(), "v4");
    }

    #[test]
    fn test_scan_sha_without_comment() {
        let temp_dir = TempDir::new().unwrap();
        let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123def456
";
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let parser = FileWorkflowScanner::new(temp_dir.path());
        let action_set = parser.scan(&workflow_path).unwrap();

        let versions = action_set.versions_for(&ActionId::from("actions/checkout"));
        // Should use SHA as version when no comment
        assert_eq!(versions[0].as_str(), "abc123def456");
    }

    #[test]
    fn test_scan_real_world_format() {
        let temp_dir = TempDir::new().unwrap();
        let content = "on:
  pull_request:

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@8e8c483db84b4bee98b60c0593521ed34d9990e8 # v6.0.1

      - name: Login
        uses: docker/login-action@5e57cd118135c172c3672efd75eb46360885c0ef # v3.6.0
";
        let workflow_path = create_test_workflow(temp_dir.path(), "test.yml", content);

        let parser = FileWorkflowScanner::new(temp_dir.path());
        let action_set = parser.scan(&workflow_path).unwrap();

        let checkout_id = ActionId::from("actions/checkout");
        let versions = action_set.versions_for(&checkout_id);
        assert_eq!(versions[0].as_str(), "v6.0.1");
        assert_eq!(
            action_set.sha_for(&checkout_id).map(CommitSha::as_str),
            Some("8e8c483db84b4bee98b60c0593521ed34d9990e8")
        );

        let login_id = ActionId::from("docker/login-action");
        let versions = action_set.versions_for(&login_id);
        assert_eq!(versions[0].as_str(), "v3.6.0");
        assert_eq!(
            action_set.sha_for(&login_id).map(CommitSha::as_str),
            Some("5e57cd118135c172c3672efd75eb46360885c0ef")
        );
    }

    #[test]
    fn test_scan_sha_none_for_tag() {
        let temp_dir = TempDir::new().unwrap();
        let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
";
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let parser = FileWorkflowScanner::new(temp_dir.path());
        let action_set = parser.scan(&workflow_path).unwrap();

        let versions = action_set.versions_for(&ActionId::from("actions/checkout"));
        assert_eq!(versions[0].as_str(), "v4");
        // No SHA when using a tag without comment
        assert!(
            action_set
                .sha_for(&ActionId::from("actions/checkout"))
                .is_none()
        );
    }

    #[test]
    fn test_scan_sha_none_for_short_ref() {
        let temp_dir = TempDir::new().unwrap();
        // Short SHA (not 40 chars) with comment
        let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123 # v4
";
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let parser = FileWorkflowScanner::new(temp_dir.path());
        let action_set = parser.scan(&workflow_path).unwrap();

        let versions = action_set.versions_for(&ActionId::from("actions/checkout"));
        assert_eq!(versions[0].as_str(), "v4");
        // Short refs are not treated as SHAs
        assert!(
            action_set
                .sha_for(&ActionId::from("actions/checkout"))
                .is_none()
        );
    }

    #[test]
    fn test_update_workflow_uses_commit_sha() {
        let temp_dir = TempDir::new().unwrap();
        let content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
";
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let writer = FileWorkflowUpdater::new(temp_dir.path());
        let mut actions = HashMap::new();
        // Simulate the format from lock.build_update_map(): "SHA # version"
        actions.insert(
            ActionId::from("actions/checkout"),
            "abc123def456 # v4".to_string(),
        );

        let result = writer.update_workflow(&workflow_path, &actions).unwrap();

        assert_eq!(result.changes.len(), 1);

        // Verify the workflow was updated with the SHA, not the tag
        let updated = fs::read_to_string(&workflow_path).unwrap();
        assert!(
            updated.contains("actions/checkout@abc123def456 # v4"),
            "Expected SHA with comment, got: {updated}"
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
        let content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3 # v3
      - uses: actions/setup-node@old_sha # v2
";
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let writer = FileWorkflowUpdater::new(temp_dir.path());
        let mut actions = HashMap::new();
        // Update both actions with new SHAs
        actions.insert(
            ActionId::from("actions/checkout"),
            "abc123def456 # v4".to_string(),
        );
        actions.insert(
            ActionId::from("actions/setup-node"),
            "xyz789012345 # v3".to_string(),
        );

        let result = writer.update_workflow(&workflow_path, &actions).unwrap();

        assert_eq!(result.changes.len(), 2);

        // Verify no duplicate comments
        let updated = fs::read_to_string(&workflow_path).unwrap();

        // Should have the new SHA with new comment
        assert!(
            updated.contains("actions/checkout@abc123def456 # v4"),
            "Expected new SHA with comment, got: {updated}"
        );

        // Should NOT have duplicate comments like "# v4 # v3"
        assert!(
            !updated.contains("# v4 # v3"),
            "Found duplicate comment in: {updated}"
        );
        assert!(
            !updated.contains("# v3 # v3"),
            "Found duplicate comment in: {updated}"
        );

        // Verify setup-node was also updated correctly
        assert!(
            updated.contains("actions/setup-node@xyz789012345 # v3"),
            "Expected setup-node with new SHA and comment, got: {updated}"
        );
        assert!(
            !updated.contains("# v3 # v2"),
            "Found duplicate comment in: {updated}"
        );
    }
}

use crate::domain::{ActionId, UpdateResult, WorkflowError};
use glob::glob;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
            for path in glob(&pattern)
                .map_err(|e| WorkflowError::ScanFailed {
                    reason: e.to_string(),
                })?
                .flatten()
            {
                workflows.push(path);
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
        FileWorkflowUpdater::update_workflow_internal(workflow_path, actions)
    }

    /// Internal implementation returning `WorkflowError` directly.
    fn update_workflow_internal(
        workflow_path: &Path,
        actions: &HashMap<ActionId, String>,
    ) -> Result<UpdateResult, WorkflowError> {
        let content =
            fs::read_to_string(workflow_path).map_err(|source| WorkflowError::ScanFailed {
                reason: format!("failed to read {}: {}", workflow_path.display(), source),
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
                Regex::new(&pattern)
                    .map_err(|e| WorkflowError::UpdateFailed {
                        path: String::new(),
                        reason: e.to_string(),
                    })
                    .map(|re| (re, replacement, change_label))
            })
            .collect::<Result<_, WorkflowError>>()?;

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
            fs::write(workflow_path, &updated_content).map_err(|source| {
                WorkflowError::UpdateFailed {
                    path: workflow_path.to_string_lossy().to_string(),
                    reason: format!("write error: {source}"),
                }
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

#[cfg(test)]
mod tests {
    use super::FileWorkflowUpdater;
    use crate::domain::ActionId;
    use std::collections::HashMap;
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
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

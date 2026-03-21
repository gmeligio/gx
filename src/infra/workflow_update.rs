use crate::domain::action::identity::ActionId;
use crate::domain::action::resolved::ResolvedAction;
use crate::domain::diff::WorkflowPatch;
use crate::domain::workflow::{Error as WorkflowError, UpdateResult};
use glob::glob;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Format a `ResolvedAction` into the workflow ref string.
///
/// This is the **single place** where `"SHA # version"` formatting exists.
fn format_uses_ref(action: &ResolvedAction) -> String {
    match &action.version {
        Some(v) => format!("{} # {v}", action.sha),
        None => action.sha.to_string(),
    }
}

/// Writer for updating action versions in workflow files.
pub struct WorkflowWriter {
    /// Path to the `.github/workflows` directory.
    workflows_dir: PathBuf,
}

impl WorkflowWriter {
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

    /// Apply a set of workflow patches, writing pin changes to workflow files.
    ///
    /// # Errors
    ///
    /// Returns an error if any workflow file cannot be updated.
    pub fn apply_patches(
        &self,
        patches: &[WorkflowPatch],
    ) -> Result<Vec<UpdateResult>, WorkflowError> {
        let mut results = Vec::new();
        for patch in patches {
            let actions = Self::pins_to_map(&patch.pins);
            let result = Self::update_workflow_internal(&patch.path, &actions)?;
            if !result.changes.is_empty() {
                results.push(result);
            }
        }
        Ok(results)
    }

    /// Update all workflow files with the same set of pins.
    ///
    /// # Errors
    ///
    /// Returns an error if any workflow file cannot be processed.
    pub fn update_all_with_pins(
        &self,
        pins: &[ResolvedAction],
    ) -> Result<Vec<UpdateResult>, WorkflowError> {
        let actions = Self::pins_to_map(pins);
        let workflows = self.find_workflows()?;
        let mut results = Vec::new();

        for workflow in workflows {
            let result = Self::update_workflow_internal(&workflow, &actions)?;
            if !result.changes.is_empty() {
                results.push(result);
            }
        }

        Ok(results)
    }

    /// Convert `ResolvedAction` pins to a `HashMap` for the internal update logic.
    fn pins_to_map(pins: &[ResolvedAction]) -> HashMap<ActionId, String> {
        pins.iter()
            .map(|pin| (pin.id.clone(), format_uses_ref(pin)))
            .collect()
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

        let mut updated_content = content;
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
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::WorkflowWriter;
    use crate::domain::action::identity::{ActionId, CommitSha, Version};
    use crate::domain::action::resolved::ResolvedAction;
    use crate::domain::diff::WorkflowPatch;
    use std::fs;
    use std::io::Write as _;
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
    fn apply_patches_updates_workflow() {
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

        let writer = WorkflowWriter::new(temp_dir.path());
        let patches = vec![WorkflowPatch {
            path: workflow_path.clone(),
            pins: vec![ResolvedAction {
                id: ActionId::from("actions/checkout"),
                sha: CommitSha::from("abc123def456"),
                version: Some(Version::from("v4")),
            }],
        }];

        let results = writer.apply_patches(&patches).unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].changes[0].contains("actions/checkout@abc123def456 # v4"));

        let updated_workflow = fs::read_to_string(&workflow_path).unwrap();
        assert!(updated_workflow.contains("actions/checkout@abc123def456 # v4"));
        assert!(updated_workflow.contains("actions/setup-node@v3")); // unchanged
    }

    #[test]
    fn apply_patches_uses_commit_sha_with_comment() {
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

        let writer = WorkflowWriter::new(temp_dir.path());
        let patches = vec![WorkflowPatch {
            path: workflow_path.clone(),
            pins: vec![ResolvedAction {
                id: ActionId::from("actions/checkout"),
                sha: CommitSha::from("abc123def456"),
                version: Some(Version::from("v4")),
            }],
        }];

        let results = writer.apply_patches(&patches).unwrap();

        assert_eq!(results.len(), 1);

        // Verify the workflow was updated with the SHA and comment
        let updated = fs::read_to_string(&workflow_path).unwrap();
        assert!(
            updated.contains("actions/checkout@abc123def456 # v4"),
            "Expected SHA with comment, got: {updated}"
        );
    }

    #[test]
    fn apply_patches_no_duplicate_comments() {
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

        let writer = WorkflowWriter::new(temp_dir.path());
        let patches = vec![WorkflowPatch {
            path: workflow_path.clone(),
            pins: vec![
                ResolvedAction {
                    id: ActionId::from("actions/checkout"),
                    sha: CommitSha::from("abc123def456"),
                    version: Some(Version::from("v4")),
                },
                ResolvedAction {
                    id: ActionId::from("actions/setup-node"),
                    sha: CommitSha::from("xyz789012345"),
                    version: Some(Version::from("v3")),
                },
            ],
        }];

        let results = writer.apply_patches(&patches).unwrap();

        assert_eq!(results.len(), 1);

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

    #[test]
    fn format_uses_ref_bare_sha() {
        let action = ResolvedAction {
            id: ActionId::from("actions/checkout"),
            sha: CommitSha::from("abc123"),
            version: None,
        };
        assert_eq!(super::format_uses_ref(&action), "abc123");
    }

    #[test]
    fn format_uses_ref_with_version() {
        let action = ResolvedAction {
            id: ActionId::from("actions/checkout"),
            sha: CommitSha::from("abc123"),
            version: Some(Version::from("v4.2.1")),
        };
        assert_eq!(super::format_uses_ref(&action), "abc123 # v4.2.1");
    }
}

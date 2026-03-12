use crate::domain::action::uses_ref::UsesRef;
use crate::domain::workflow::Error as WorkflowError;
use glob::glob;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Internal I/O errors for workflow operations
#[derive(Debug, Error)]
enum IoWorkflowError {
    #[error("glob pattern error")]
    Glob(#[from] glob::PatternError),

    #[error("read error: {}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("YAML parse error: {}", path.display())]
    Parse {
        path: PathBuf,
        source: Box<serde_saphyr::Error>,
    },

    #[error("regex error")]
    Regex(#[from] regex::Error),
}

impl From<IoWorkflowError> for WorkflowError {
    fn from(err: IoWorkflowError) -> Self {
        match err {
            IoWorkflowError::Glob(e) => WorkflowError::ScanFailed {
                reason: e.to_string(),
            },
            IoWorkflowError::Read { path, source } => WorkflowError::ScanFailed {
                reason: format!("failed to read {}: {}", path.display(), source),
            },
            IoWorkflowError::Parse { path, source } => WorkflowError::ParseFailed {
                path: path.to_string_lossy().to_string(),
                reason: source.to_string(),
            },
            IoWorkflowError::Regex(e) => WorkflowError::UpdateFailed {
                path: String::new(),
                reason: e.to_string(),
            },
        }
    }
}

/// Action data extracted from a workflow file.
/// Call `uses_ref.interpret()` to get domain types.
#[derive(Debug, Clone)]
struct ExtractedAction {
    uses_ref: UsesRef,
    location: crate::domain::workflow_actions::Location,
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

/// Find all workflow files in a workflows directory.
///
/// # Errors
///
/// Returns an error if the glob pattern is invalid or file access fails.
fn find_workflow_files(workflows_dir: &Path) -> Result<Vec<PathBuf>, IoWorkflowError> {
    let mut workflows = Vec::new();

    for extension in &["yml", "yaml"] {
        let pattern = workflows_dir
            .join(format!("*.{extension}"))
            .to_string_lossy()
            .to_string();

        for entry in glob(&pattern)? {
            match entry {
                Ok(path) => workflows.push(path),
                Err(_e) => {}
            }
        }
    }

    Ok(workflows)
}

/// Parser for extracting action information from workflow files
pub struct FileScanner {
    repo_root: PathBuf,
    workflows_dir: PathBuf,
}

impl FileScanner {
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
        find_workflow_files(&self.workflows_dir).map_err(Into::into)
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
    ) -> Result<Vec<ExtractedAction>, IoWorkflowError> {
        let content =
            fs::read_to_string(workflow_path).map_err(|source| IoWorkflowError::Read {
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
            serde_saphyr::from_str(&content).map_err(|source| IoWorkflowError::Parse {
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
                        location: crate::domain::workflow_actions::Location {
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
    pub fn scan_file(
        &self,
        workflow_path: &Path,
    ) -> Result<crate::domain::workflow_actions::ActionSet, WorkflowError> {
        let rel = self.rel_path(workflow_path);
        let actions = Self::extract_actions(workflow_path, &rel)?;
        let mut action_set = crate::domain::workflow_actions::ActionSet::new();
        for action in &actions {
            action_set.add(&action.uses_ref.interpret());
        }
        Ok(action_set)
    }

    /// Convert extracted actions from a single file into `LocatedAction` items.
    fn located_from_file(
        workflow_path: &Path,
        workflow_rel_path: &str,
    ) -> Result<Vec<crate::domain::workflow_actions::Located>, WorkflowError> {
        let actions =
            Self::extract_actions(workflow_path, workflow_rel_path).map_err(WorkflowError::from)?;
        Ok(actions
            .into_iter()
            .map(|action| {
                let interpreted = action.uses_ref.interpret();
                crate::domain::workflow_actions::Located {
                    id: interpreted.id,
                    version: interpreted.version,
                    sha: interpreted.sha,
                    location: action.location,
                }
            })
            .collect())
    }
}

impl crate::domain::workflow::Scanner for FileScanner {
    fn scan(
        &self,
    ) -> Box<
        dyn Iterator<Item = Result<crate::domain::workflow_actions::Located, WorkflowError>> + '_,
    > {
        let workflows = match self.find_workflows() {
            Ok(w) => w,
            Err(e) => return Box::new(std::iter::once(Err(e))),
        };

        Box::new(workflows.into_iter().flat_map(move |workflow_path| {
            let rel = self.rel_path(&workflow_path);
            match Self::located_from_file(&workflow_path, &rel) {
                Ok(actions) => Box::new(actions.into_iter().map(Ok))
                    as Box<
                        dyn Iterator<
                            Item = Result<crate::domain::workflow_actions::Located, WorkflowError>,
                        >,
                    >,
                Err(e) => Box::new(std::iter::once(Err(e))),
            }
        }))
    }

    fn scan_paths(&self) -> Box<dyn Iterator<Item = Result<PathBuf, WorkflowError>> + '_> {
        match self.find_workflows() {
            Ok(paths) => Box::new(paths.into_iter().map(Ok)),
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

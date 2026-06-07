use crate::domain::action::uses_ref::UsesRef;
use crate::domain::workflow::Error as WorkflowError;
use crate::domain::workflow_actions::{JobId, StepIndex, WorkflowPath};
use crate::domain::workflow_parsed::Parsed;
use crate::regex::static_regex;
use glob::glob;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

// Splits an action reference into `owner/repo` (or path) and its `@ref`.
static_regex!(USES_RE, r"^([^@\s]+)@([^\s#]+)");

/// Internal I/O errors for workflow operations.
#[derive(Debug, Error)]
enum IoWorkflowError {
    /// A glob pattern could not be compiled.
    #[error("glob pattern error")]
    Glob(#[from] glob::PatternError),

    /// A workflow file could not be read from disk.
    #[error("read error: {}", path.display())]
    Read {
        /// The file path that could not be read.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// A workflow file could not be parsed as YAML.
    #[error("YAML parse error: {}", path.display())]
    Parse {
        /// The file path that could not be parsed.
        path: PathBuf,
        /// The underlying YAML parse error.
        source: Box<serde_saphyr::Error>,
    },

    /// A regex pattern could not be compiled.
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
    /// The parsed `uses:` reference from the workflow step.
    uses_ref: UsesRef,
    /// The workflow/job/step location where this action was found.
    location: crate::domain::workflow_actions::Location,
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

/// Parser for extracting action information from workflow files.
pub struct FileScanner {
    /// Root directory of the repository.
    repo_root: PathBuf,
    /// Path to the `.github/workflows` directory.
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
    fn rel_path(&self, workflow_path: &Path) -> WorkflowPath {
        WorkflowPath::new(
            workflow_path
                .strip_prefix(&self.repo_root)
                .unwrap_or(workflow_path)
                .to_string_lossy()
                .into_owned(),
        )
    }

    /// Find all workflow files in the repository's `.github/workflows` folder.
    ///
    /// # Errors
    ///
    /// Returns an error if the glob pattern is invalid.
    pub fn find_workflows(&self) -> Result<Vec<PathBuf>, WorkflowError> {
        find_workflow_files(&self.workflows_dir).map_err(Into::into)
    }

    /// Parse a workflow file once and return both the structural `Parsed` model and
    /// the list of `uses:` action references with their location metadata.
    ///
    /// The action list is derived from `parsed.jobs[].steps[].uses`. Each step's inline
    /// version comment (e.g. `# v4`) rides on the parsed value via saphyr's
    /// `Commented<String>`, so it is read per step rather than re-scraped from the source.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed as YAML.
    fn extract_workflow(
        workflow_path: &Path,
        workflow_rel_path: &WorkflowPath,
    ) -> Result<(Parsed, Vec<ExtractedAction>), IoWorkflowError> {
        let content =
            fs::read_to_string(workflow_path).map_err(|source| IoWorkflowError::Read {
                path: workflow_path.to_path_buf(),
                source,
            })?;

        let parsed = Parsed::from_yaml(workflow_rel_path.clone(), &content).map_err(|source| {
            IoWorkflowError::Parse {
                path: workflow_path.to_path_buf(),
                source,
            }
        })?;

        let mut actions = Vec::new();

        for job in &parsed.jobs {
            for (step_idx, step) in job.steps.iter().enumerate() {
                let Some(uses) = step.uses_ref() else {
                    continue;
                };
                let Some(cap) = USES_RE.captures(uses) else {
                    continue;
                };
                let action_name = cap[1].to_string();
                let uses_ref = cap[2].to_string();

                if action_name.starts_with('.') || action_name.starts_with("docker://") {
                    continue;
                }

                let comment = step.uses_comment().map(ToOwned::to_owned);

                actions.push(ExtractedAction {
                    uses_ref: UsesRef::new(action_name, uses_ref, comment),
                    location: crate::domain::workflow_actions::Location {
                        workflow: workflow_rel_path.clone(),
                        job: Some(JobId::from(job.id.clone())),
                        step: StepIndex::try_from(step_idx).ok(),
                    },
                });
            }
        }

        Ok((parsed, actions))
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
        let (_, actions) = Self::extract_workflow(workflow_path, &rel)?;
        let mut action_set = crate::domain::workflow_actions::ActionSet::new();
        for action in &actions {
            action_set.add(&action.uses_ref.interpret());
        }
        Ok(action_set)
    }

    /// Convert extracted actions from a single file into `LocatedAction` items.
    fn located_from_file(
        workflow_path: &Path,
        workflow_rel_path: &WorkflowPath,
    ) -> Result<Vec<crate::domain::workflow_actions::Located>, WorkflowError> {
        let (_, actions) = Self::extract_workflow(workflow_path, workflow_rel_path)
            .map_err(WorkflowError::from)?;
        Ok(actions
            .into_iter()
            .map(|action| crate::domain::workflow_actions::Located {
                action: action.uses_ref.interpret(),
                location: action.location,
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
        type LocatedIter = Box<
            dyn Iterator<Item = Result<crate::domain::workflow_actions::Located, WorkflowError>>,
        >;

        let workflows = match self.find_workflows() {
            Ok(w) => w,
            Err(e) => return Box::new(std::iter::once(Err(e))),
        };

        Box::new(workflows.into_iter().flat_map(move |workflow_path| {
            let rel = self.rel_path(&workflow_path);
            match Self::located_from_file(&workflow_path, &rel) {
                Ok(actions) => {
                    let iter: LocatedIter = Box::new(actions.into_iter().map(Ok));
                    iter
                }
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

    fn scan_all_with_parsed(
        &self,
    ) -> Result<(Vec<crate::domain::workflow_actions::Located>, Vec<Parsed>), WorkflowError> {
        let workflows = self.find_workflows()?;
        let mut located = Vec::new();
        let mut parsed = Vec::new();
        for workflow_path in workflows {
            let rel = self.rel_path(&workflow_path);
            let (p, actions) =
                Self::extract_workflow(&workflow_path, &rel).map_err(WorkflowError::from)?;
            located.extend(
                actions
                    .into_iter()
                    .map(|a| crate::domain::workflow_actions::Located {
                        action: a.uses_ref.interpret(),
                        location: a.location,
                    }),
            );
            parsed.push(p);
        }
        Ok((located, parsed))
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
#[path = "tests.rs"]
mod tests;

use super::discovery;
use crate::domain::action::uses_ref::UsesRef;
use crate::domain::workflow::Error as WorkflowError;
use crate::domain::workflow_actions::{JobId, StepIndex, WorkflowPath};
use crate::domain::workflow_parsed::{FileKind, Parsed, Step};
use crate::regex::static_regex;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

// Splits an action reference into `owner/repo` (or path) and its `@ref`.
static_regex!(USES_RE, r"^([^@\s]+)@([^\s#]+)");

/// Errors from reading one managed file. Discovery errors are raised in
/// [`super::discovery`], which owns the globbing.
#[derive(Debug, Error)]
enum IoWorkflowError {
    /// A managed file could not be read from disk.
    #[error("read error: {}", path.display())]
    Read {
        /// The file path that could not be read.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// A managed file could not be parsed as YAML.
    #[error("YAML parse error: {}", path.display())]
    Parse {
        /// The file path that could not be parsed.
        path: PathBuf,
        /// The underlying YAML parse error.
        source: Box<serde_saphyr::Error>,
    },
}

impl From<IoWorkflowError> for WorkflowError {
    fn from(err: IoWorkflowError) -> Self {
        match err {
            IoWorkflowError::Read { path, source } => WorkflowError::ScanFailed {
                reason: format!("failed to read {}: {}", path.display(), source),
            },
            IoWorkflowError::Parse { path, source } => WorkflowError::ParseFailed {
                path: path.to_string_lossy().to_string(),
                reason: source.to_string(),
            },
        }
    }
}

/// Action data extracted from a managed file.
/// Call `uses_ref.interpret()` to get domain types.
#[derive(Debug, Clone)]
struct ExtractedAction {
    /// The parsed `uses:` reference from the step.
    uses_ref: UsesRef,
    /// The workflow/job/step location where this action was found.
    location: crate::domain::workflow_actions::Location,
}

/// Pull the registry `uses:` references out of one step list, tagging each with its
/// location. `job` is `None` for a composite action's steps, which belong to no job.
///
/// Local references (`./…`) and `docker://` references are skipped: neither is a
/// registry action with a version to manage.
fn extract_steps(
    steps: &[Step],
    workflow_rel_path: &WorkflowPath,
    job: Option<&JobId>,
    out: &mut Vec<ExtractedAction>,
) {
    for (step_idx, step) in steps.iter().enumerate() {
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

        out.push(ExtractedAction {
            uses_ref: UsesRef::new(action_name, uses_ref, comment),
            location: crate::domain::workflow_actions::Location {
                workflow: workflow_rel_path.clone(),
                job: job.cloned(),
                step: StepIndex::try_from(step_idx).ok(),
                line: step.uses_line(),
            },
        });
    }
}

/// Parser for extracting action information from managed files.
pub struct FileScanner {
    /// Root directory of the repository.
    repo_root: PathBuf,
}

impl FileScanner {
    #[must_use]
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
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

    /// Find every managed file in the repository — workflows and action definitions —
    /// in discovery order. Callers outside this module reach this through the
    /// `Scanner::find_workflow_paths` trait method.
    ///
    /// # Errors
    ///
    /// Returns an error if the glob pattern is invalid.
    fn find_managed_paths(&self) -> Result<Vec<PathBuf>, WorkflowError> {
        discovery::managed_paths(&self.repo_root)
    }

    /// Find every managed file with the schema each one follows.
    ///
    /// # Errors
    ///
    /// Returns an error if the glob pattern is invalid.
    fn find_managed(&self) -> Result<Vec<discovery::ManagedFile>, WorkflowError> {
        discovery::managed_files(&self.repo_root)
    }

    /// Parse a managed file once and return both the structural `Parsed` model and
    /// the list of `uses:` action references with their location metadata, each
    /// carrying its inline version comment (e.g. `# v4`).
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed as YAML.
    fn extract_workflow(
        workflow_path: &Path,
        workflow_rel_path: &WorkflowPath,
        kind: FileKind,
    ) -> Result<(Parsed, Vec<ExtractedAction>), IoWorkflowError> {
        let content =
            fs::read_to_string(workflow_path).map_err(|source| IoWorkflowError::Read {
                path: workflow_path.to_path_buf(),
                source,
            })?;

        let parsed =
            Parsed::parse(workflow_rel_path.clone(), kind, &content).map_err(|source| {
                IoWorkflowError::Parse {
                    path: workflow_path.to_path_buf(),
                    source,
                }
            })?;

        let mut actions = Vec::new();

        // Only the step lookup differs between the schemas; extraction below is shared.
        match kind {
            FileKind::Workflow => {
                for job in &parsed.jobs {
                    let job_id = JobId::from(job.id.clone());
                    extract_steps(&job.steps, workflow_rel_path, Some(&job_id), &mut actions);
                }
            }
            FileKind::ActionDefinition => {
                extract_steps(&parsed.steps, workflow_rel_path, None, &mut actions);
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
        let (_, actions) =
            Self::extract_workflow(workflow_path, &rel, FileKind::of_path(workflow_path))?;
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
        kind: FileKind,
    ) -> Result<Vec<crate::domain::workflow_actions::Located>, WorkflowError> {
        let (_, actions) = Self::extract_workflow(workflow_path, workflow_rel_path, kind)
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

        let files = match self.find_managed() {
            Ok(w) => w,
            Err(e) => return Box::new(std::iter::once(Err(e))),
        };

        Box::new(files.into_iter().flat_map(move |file| {
            let rel = self.rel_path(&file.path);
            match Self::located_from_file(&file.path, &rel, file.kind) {
                Ok(actions) => {
                    let iter: LocatedIter = Box::new(actions.into_iter().map(Ok));
                    iter
                }
                Err(e) => Box::new(std::iter::once(Err(e))),
            }
        }))
    }

    fn scan_paths(&self) -> Box<dyn Iterator<Item = Result<PathBuf, WorkflowError>> + '_> {
        match self.find_managed_paths() {
            Ok(paths) => Box::new(paths.into_iter().map(Ok)),
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }

    fn scan_all_with_parsed(
        &self,
    ) -> Result<(Vec<crate::domain::workflow_actions::Located>, Vec<Parsed>), WorkflowError> {
        let files = self.find_managed()?;
        let mut located = Vec::new();
        let mut parsed = Vec::new();
        for file in files {
            let rel = self.rel_path(&file.path);
            let (p, actions) =
                Self::extract_workflow(&file.path, &rel, file.kind).map_err(WorkflowError::from)?;
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

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
#[path = "composite_tests.rs"]
mod composite_tests;

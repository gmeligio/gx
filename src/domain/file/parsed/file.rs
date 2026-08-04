//! The parsed form of a managed file, as a sum over the schemas gx reads.
//!
//! A workflow and an action definition share almost no structure: only the workflow has
//! `on:`, `permissions:`, `concurrency:` and jobs, and only a composite action has
//! `runs.steps`. Modelling them as one struct made "this schema has no jobs" and "this
//! workflow declares no jobs" the same value, so a rule reading `jobs` on an action
//! definition compiled and silently found nothing.

use super::kind::FileKind;
use super::{Concurrency, Defaults, Job, Permissions, Step, Trigger};
use crate::domain::file::site::WorkflowPath;

/// A parsed workflow. Structural fields only — `name`, `env`, `runs-on` and friends are
/// intentionally not captured.
#[derive(Debug, Clone)]
pub struct ParsedWorkflow {
    /// Repo-relative path of the file this was parsed from.
    pub path: WorkflowPath,
    /// The `on:` triggers. Empty when the workflow declares none.
    pub on: Vec<Trigger>,
    /// The workflow-level `permissions:` block, if declared.
    pub permissions: Option<Permissions>,
    /// The workflow-level `concurrency:` block, if declared.
    pub concurrency: Option<Concurrency>,
    /// The workflow-level `defaults:` block. Lowest-precedence source for a step's
    /// effective shell (below the step's own `shell:` and the job's `defaults`).
    pub defaults: Option<Defaults>,
    /// The workflow's jobs, in `jobs:` key order.
    pub jobs: Vec<Job>,
}

impl ParsedWorkflow {
    /// True if any trigger in `on` matches.
    #[must_use]
    pub fn has_trigger(&self, t: &Trigger) -> bool {
        self.on.iter().any(|x| x == t)
    }
}

/// A parsed action definition.
#[derive(Debug, Clone)]
pub struct ParsedAction {
    /// Repo-relative path of the file this was parsed from.
    pub path: WorkflowPath,
    /// The steps of the `runs:` block. Empty unless `runs.using` is `composite` — a
    /// `node20` or `docker` action has `main`/`image` instead and no `uses:` steps to
    /// manage.
    pub steps: Vec<Step>,
}

/// A parsed managed file: whichever schema discovery said it follows.
#[derive(Debug, Clone)]
pub enum Parsed {
    /// A workflow, holding `jobs.<id>.steps`.
    Workflow(ParsedWorkflow),
    /// An action definition, holding `runs.steps`.
    Action(ParsedAction),
}

// The parsing constructors live beside the wire-format structs in the parent module; the
// accessors below are the whole of this type's read surface.
impl Parsed {
    /// Repo-relative path of the file, whichever schema it follows.
    #[must_use]
    pub fn path(&self) -> &WorkflowPath {
        match self {
            Self::Workflow(w) => &w.path,
            Self::Action(a) => &a.path,
        }
    }

    /// Which schema this file follows.
    #[must_use]
    pub fn kind(&self) -> FileKind {
        match self {
            Self::Workflow(_) => FileKind::Workflow,
            Self::Action(_) => FileKind::ActionDefinition,
        }
    }

    /// The workflow view, or `None` for an action definition.
    ///
    /// This is the total function the workflow-schema lint rules narrow through: they take
    /// [`ParsedWorkflow`], so a caller cannot hand them an action definition by forgetting
    /// a filter.
    #[must_use]
    pub fn as_workflow(&self) -> Option<&ParsedWorkflow> {
        match self {
            Self::Workflow(w) => Some(w),
            Self::Action(_) => None,
        }
    }
}

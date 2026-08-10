//! The diagnostic record and the ignore matchers that decide whether a diagnostic
//! survives a user's `ignore` configuration.
//!
//! Generic over the rule-identity type so that each command supplies its own closed
//! set of rule names while sharing one diagnostic vocabulary.

use crate::config::IgnoreTarget;
use crate::config::Level;
use crate::domain::action::identity::ActionId;
use crate::domain::file::actions::Located as LocatedAction;
use crate::domain::file::site::{JobId, StepIndex, WorkflowPath};

/// A single diagnostic reported by a rule.
///
/// `Id` is the reporting command's rule-identity type (e.g. `lint::RuleName`).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Diagnostic<Id> {
    /// Identity of the rule that produced this diagnostic.
    pub rule: Id,
    /// Severity level.
    pub level: Level,
    /// Human-readable message.
    pub message: String,
    /// Optional workflow file path where the issue was found.
    pub workflow: Option<WorkflowPath>,
    /// Optional job id (set by rules whose diagnostics target a specific job).
    pub job: Option<JobId>,
    /// Optional 0-based step index (set by step-scoped diagnostics).
    pub step: Option<StepIndex>,
    /// Optional 1-based source line of the offending `uses:` scalar. Set by rules whose
    /// diagnostic maps to a single workflow line; left `None` for manifest-level or
    /// whole-file diagnostics that have no single line to point at.
    pub line: Option<u32>,
}

impl<Id> Diagnostic<Id> {
    /// Create a new diagnostic.
    pub fn new<S: Into<String>>(rule: Id, level: Level, message: S) -> Self {
        Self {
            rule,
            level,
            message: message.into(),
            workflow: None,
            job: None,
            step: None,
            line: None,
        }
    }

    /// Set the workflow field.
    #[must_use]
    pub fn with_workflow(mut self, workflow: WorkflowPath) -> Self {
        self.workflow = Some(workflow);
        self
    }

    /// Set the job field.
    #[must_use]
    pub fn with_job(mut self, job: JobId) -> Self {
        self.job = Some(job);
        self
    }

    /// Set the step field.
    #[must_use]
    pub fn with_step(mut self, step: StepIndex) -> Self {
        self.step = Some(step);
        self
    }

    /// Set the source line.
    #[must_use]
    pub fn with_line(mut self, line: Option<u32>) -> Self {
        self.line = line;
        self
    }
}

/// An `ignore` target matches only when every key it sets matches — an unset key imposes
/// nothing. Each axis below answers for one key.
///
/// A `workflow` key matches by suffix, so `ci.yml` covers `.github/workflows/ci.yml`.
fn workflow_axis(diag_workflow: Option<&WorkflowPath>, target: &IgnoreTarget) -> bool {
    let Some(target_workflow) = &target.workflow else {
        return true;
    };
    diag_workflow.is_some_and(|w| w.as_str().ends_with(target_workflow.as_str()))
}

/// A `job` key matches the diagnostic's own job. Diagnostics with no job never match one.
fn job_axis<Id>(diag: &Diagnostic<Id>, target: &IgnoreTarget) -> bool {
    let Some(target_job) = &target.job else {
        return true;
    };
    diag.job
        .as_ref()
        .is_some_and(|j| j.as_str() == target_job.as_str())
}

/// An `action` key matches the action the diagnostic is about, when one is known.
fn action_axis(diag_action: Option<&ActionId>, target: &IgnoreTarget) -> bool {
    let Some(target_action) = &target.action else {
        return true;
    };
    diag_action.is_some_and(|id| id.as_str() == target_action.as_str())
}

/// Matcher for workflow-scoped rules, which are about a file rather than an action — so an
/// `action` key never matches one and users should omit it.
pub fn matches_ignore_workflow<Id>(diag: &Diagnostic<Id>, target: &IgnoreTarget) -> bool {
    target.action.is_none()
        && workflow_axis(diag.workflow.as_ref(), target)
        && job_axis(diag, target)
}

/// Matcher for action-scoped rules, where the caller already knows which action the
/// diagnostic is about.
pub fn matches_ignore_action<Id>(
    diag: &Diagnostic<Id>,
    target: &IgnoreTarget,
    action: &LocatedAction,
) -> bool {
    diag.workflow.is_some()
        && action_axis(Some(&action.action.id), target)
        && workflow_axis(diag.workflow.as_ref(), target)
        && target.job.is_none()
}

/// Matcher for aggregate phases, which have no action in hand and must recover it from the
/// diagnostic's workflow.
pub fn matches_ignore<Id>(
    diag: &Diagnostic<Id>,
    target: &IgnoreTarget,
    located_actions: &[LocatedAction],
) -> bool {
    let Some(diag_workflow) = &diag.workflow else {
        return false;
    };
    let diag_action = located_actions
        .iter()
        .find(|loc| loc.site.file == *diag_workflow)
        .map(|loc| &loc.action.id);

    action_axis(diag_action, target)
        && workflow_axis(Some(diag_workflow), target)
        && target.job.is_none()
}

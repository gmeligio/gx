//! The diagnostic record and the ignore matchers that decide whether a diagnostic
//! survives a user's `ignore` configuration.
//!
//! Nothing here is specific to any one command: a command supplies its rules, and the
//! record, the matchers, and the report in `super::report` are shared.

use crate::config::IgnoreTarget;
use crate::config::Level;
use crate::domain::file::actions::Located as LocatedAction;
use crate::domain::file::site::{JobId, StepIndex, WorkflowPath};

use super::RuleName;

/// A single diagnostic reported by a rule.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Diagnostic {
    /// Identity of the rule that produced this diagnostic.
    pub rule: RuleName,
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

impl Diagnostic {
    /// Create a new diagnostic.
    pub fn new<S: Into<String>>(rule: RuleName, level: Level, message: S) -> Self {
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

/// True when the target's `workflow` key (if any) matches the diagnostic's workflow by
/// suffix. A `None` target workflow always matches; a `Some` requires both a diagnostic
/// workflow and a suffix match. Shared by all three ignore matchers below, which differ
/// only in how they handle the `action` and `job` axes.
fn workflow_matches(diag_workflow: Option<&WorkflowPath>, target: &IgnoreTarget) -> bool {
    let Some(target_workflow) = &target.workflow else {
        return true;
    };
    diag_workflow.is_some_and(|w| w.as_str().ends_with(target_workflow.as_str()))
}

/// Ignore matcher for workflow-scoped diagnostics. Uses the diagnostic's structural
/// fields (workflow, job) directly. The `action` key is meaningless for these rules,
/// so an ignore target that specifies `action` will NOT match — users should omit it.
pub(crate) fn matches_ignore_workflow(diag: &Diagnostic, target: &IgnoreTarget) -> bool {
    if target.action.is_some() {
        return false;
    }
    if !workflow_matches(diag.workflow.as_ref(), target) {
        return false;
    }
    if let Some(target_job) = &target.job {
        let Some(diag_job) = &diag.job else {
            return false;
        };
        if diag_job.as_str() != target_job.as_str() {
            return false;
        }
    }
    true
}

/// Check if a diagnostic matches an ignore target using the current action context.
pub(crate) fn matches_ignore_action(
    diag: &Diagnostic,
    target: &IgnoreTarget,
    action: &LocatedAction,
) -> bool {
    if diag.workflow.is_none() {
        return false;
    }

    if let Some(target_action) = &target.action
        && action.action.id.as_str() != target_action.as_str()
    {
        return false;
    }

    if !workflow_matches(diag.workflow.as_ref(), target) {
        return false;
    }

    if target.job.is_some() {
        return false;
    }

    true
}

/// Ignore matcher for aggregate phases that lack a per-action `LocatedAction` to scope
/// against. Resolves the diagnostic's workflow against the workflow set and applies
/// intersection semantics across action / workflow.
pub(crate) fn matches_ignore(
    diag: &Diagnostic,
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

    if let Some(target_action) = &target.action {
        if let Some(matched_action) = diag_action {
            if matched_action.as_str() != target_action.as_str() {
                return false;
            }
        } else {
            return false;
        }
    }

    if !workflow_matches(Some(diag_workflow), target) {
        return false;
    }

    if target.job.is_some() {
        return false;
    }

    true
}

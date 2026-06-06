//! Rule identity, diagnostic shape, shared context, and the ignore-matching helpers
//! the runner uses to apply per-rule `ignore` lists. Kept separate from `command.rs`
//! so the runner stays focused on phase orchestration.

use super::report::Report;
use crate::config::{IgnoreTarget, Level, Lint as LintConfig};
use crate::domain::lock::Lock;
use crate::domain::manifest::Manifest;
use crate::domain::workflow_actions::{
    ActionSet as WorkflowActionSet, JobId, Located as LocatedAction, StepIndex, WorkflowPath,
};
use crate::domain::workflow_parsed::Parsed as ParsedWorkflow;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Canonical identifier for a lint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleName {
    ShaMismatch,
    Unpinned,
    StaleComment,
    UnsyncedManifest,
    MissingPermissions,
    ExcessivePermissions,
    DangerousTrigger,
    PrHeadCheckout,
    MissingConcurrency,
    UnprotectedSecrets,
    DanglingReference,
    InvalidExpression,
    RunShellcheck,
}

impl std::fmt::Display for RuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShaMismatch => write!(f, "sha-mismatch"),
            Self::Unpinned => write!(f, "unpinned"),
            Self::StaleComment => write!(f, "stale-comment"),
            Self::UnsyncedManifest => write!(f, "unsynced-manifest"),
            Self::MissingPermissions => write!(f, "missing-permissions"),
            Self::ExcessivePermissions => write!(f, "excessive-permissions"),
            Self::DangerousTrigger => write!(f, "dangerous-trigger"),
            Self::PrHeadCheckout => write!(f, "pr-head-checkout"),
            Self::MissingConcurrency => write!(f, "missing-concurrency"),
            Self::UnprotectedSecrets => write!(f, "unprotected-secrets"),
            Self::DanglingReference => write!(f, "dangling-reference"),
            Self::InvalidExpression => write!(f, "invalid-expression"),
            Self::RunShellcheck => write!(f, "run-shellcheck"),
        }
    }
}

impl FromStr for RuleName {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sha-mismatch" => Ok(Self::ShaMismatch),
            "unpinned" => Ok(Self::Unpinned),
            "stale-comment" => Ok(Self::StaleComment),
            "unsynced-manifest" => Ok(Self::UnsyncedManifest),
            "missing-permissions" => Ok(Self::MissingPermissions),
            "excessive-permissions" => Ok(Self::ExcessivePermissions),
            "dangerous-trigger" => Ok(Self::DangerousTrigger),
            "pr-head-checkout" => Ok(Self::PrHeadCheckout),
            "missing-concurrency" => Ok(Self::MissingConcurrency),
            "unprotected-secrets" => Ok(Self::UnprotectedSecrets),
            "dangling-reference" => Ok(Self::DanglingReference),
            "invalid-expression" => Ok(Self::InvalidExpression),
            "run-shellcheck" => Ok(Self::RunShellcheck),
            other => Err(format!("unrecognized rule name: {other}")),
        }
    }
}

/// A single diagnostic reported by a lint rule.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Diagnostic {
    /// Name of the rule that produced this diagnostic.
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
}

/// Context shared by all lint rules during checking.
pub struct Context<'ctx> {
    /// The manifest (gx.toml).
    pub manifest: &'ctx Manifest,
    /// The lock file (gx.lock).
    pub lock: &'ctx Lock,
    /// All located actions from scanned workflows.
    pub workflows: &'ctx [LocatedAction],
    /// Structural per-workflow parses, consumed by the workflow-security rules.
    /// Action-hygiene rules (sha-mismatch, unpinned, stale-comment, unsynced-manifest)
    /// continue to use `workflows`; this field is empty when no workflows were scanned.
    pub workflows_full: &'ctx [ParsedWorkflow],
    /// Aggregated action set from all workflows.
    pub action_set: &'ctx WorkflowActionSet,
}

/// Trait for a lint rule.
pub trait Rule {
    /// Returns the rule's name.
    fn name(&self) -> RuleName;

    /// Returns this rule's default severity level.
    fn default_level(&self) -> Level;

    /// Run the lint check and return all detected diagnostics.
    /// Rules report everything they find; filtering against ignores happens in the orchestrator.
    fn check(&self, ctx: &Context) -> Vec<Diagnostic>;
}

/// Build a `Report` from diagnostics.
#[must_use]
pub fn format_and_report(diagnostics: Vec<Diagnostic>) -> Report {
    Report::from_diagnostics(diagnostics)
}

/// Run a workflow-scoped rule. Filters its diagnostics through the per-rule `ignore`
/// list using the new workflow/job-aware matcher, applies the configured severity, and
/// pushes the survivors onto `out`.
pub(super) fn run_workflow_rule<R: Rule>(
    rule: &R,
    default_level: Level,
    ctx: &Context<'_>,
    lint_config: &LintConfig,
    out: &mut Vec<Diagnostic>,
) {
    let configured = lint_config.get_rule(rule.name(), default_level);
    if configured.level == Level::Off {
        return;
    }
    for mut diag in rule.check(ctx) {
        diag.level = configured.level;
        let ignored = configured
            .ignore
            .iter()
            .any(|target| matches_ignore_workflow(&diag, target));
        if !ignored {
            out.push(diag);
        }
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

/// Ignore matcher for workflow-security diagnostics. Uses Diagnostic's structural
/// fields (workflow, job) directly. The `action` key is meaningless for these rules,
/// so an ignore target that specifies `action` will NOT match — users should omit it.
fn matches_ignore_workflow(diag: &Diagnostic, target: &IgnoreTarget) -> bool {
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

/// Check if a per-action diagnostic is ignored via lint config.
pub(super) fn is_ignored(
    diag: &Diagnostic,
    rule_name: RuleName,
    default_level: Level,
    lint_config: &LintConfig,
    action: &LocatedAction,
) -> bool {
    lint_config
        .get_rule(rule_name, default_level)
        .ignore
        .iter()
        .any(|target| matches_ignore_action(diag, target, action))
}

/// Check if a diagnostic matches an ignore target using the current action context.
fn matches_ignore_action(diag: &Diagnostic, target: &IgnoreTarget, action: &LocatedAction) -> bool {
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

/// Ignore matcher kept for the `UnsyncedManifest` aggregate phase, which lacks a
/// per-action `LocatedAction` to scope against. Resolves the diagnostic's workflow
/// against the workflow set and applies intersection semantics across action / workflow.
pub(super) fn matches_ignore(
    diag: &Diagnostic,
    target: &IgnoreTarget,
    located_actions: &[LocatedAction],
) -> bool {
    let Some(diag_workflow) = &diag.workflow else {
        return false;
    };

    let diag_action = located_actions
        .iter()
        .find(|loc| loc.location.workflow == *diag_workflow)
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

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Diagnostic, Level, RuleName, WorkflowPath};
    use std::str::FromStr as _;

    #[test]
    fn diagnostic_can_be_created() {
        let diag = Diagnostic::new(RuleName::ShaMismatch, Level::Error, "test message");
        assert_eq!(diag.rule, RuleName::ShaMismatch);
        assert_eq!(diag.level, Level::Error);
        assert_eq!(diag.message, "test message");
        assert!(diag.workflow.is_none());
    }

    #[test]
    fn diagnostic_with_workflow() {
        let diag = Diagnostic::new(RuleName::Unpinned, Level::Warn, "test")
            .with_workflow(WorkflowPath::new(".github/workflows/ci.yml"));
        assert_eq!(
            diag.workflow,
            Some(WorkflowPath::new(".github/workflows/ci.yml"))
        );
    }

    #[test]
    fn rule_name_display_roundtrip() {
        for name in [
            RuleName::ShaMismatch,
            RuleName::Unpinned,
            RuleName::StaleComment,
            RuleName::UnsyncedManifest,
            RuleName::MissingPermissions,
            RuleName::ExcessivePermissions,
            RuleName::DangerousTrigger,
            RuleName::PrHeadCheckout,
            RuleName::MissingConcurrency,
            RuleName::UnprotectedSecrets,
            RuleName::DanglingReference,
            RuleName::InvalidExpression,
            RuleName::RunShellcheck,
        ] {
            let s = name.to_string();
            assert_eq!(RuleName::from_str(&s), Ok(name));
        }
    }

    #[test]
    fn rule_name_from_str_valid() {
        assert_eq!(
            RuleName::from_str("sha-mismatch"),
            Ok(RuleName::ShaMismatch)
        );
        assert_eq!(RuleName::from_str("unpinned"), Ok(RuleName::Unpinned));
        assert_eq!(
            RuleName::from_str("stale-comment"),
            Ok(RuleName::StaleComment)
        );
        assert_eq!(
            RuleName::from_str("unsynced-manifest"),
            Ok(RuleName::UnsyncedManifest)
        );
        assert_eq!(
            RuleName::from_str("missing-permissions"),
            Ok(RuleName::MissingPermissions)
        );
        assert_eq!(
            RuleName::from_str("excessive-permissions"),
            Ok(RuleName::ExcessivePermissions)
        );
        assert_eq!(
            RuleName::from_str("dangerous-trigger"),
            Ok(RuleName::DangerousTrigger)
        );
        assert_eq!(
            RuleName::from_str("pr-head-checkout"),
            Ok(RuleName::PrHeadCheckout)
        );
        assert_eq!(
            RuleName::from_str("missing-concurrency"),
            Ok(RuleName::MissingConcurrency)
        );
        assert_eq!(
            RuleName::from_str("unprotected-secrets"),
            Ok(RuleName::UnprotectedSecrets)
        );
        assert_eq!(
            RuleName::from_str("dangling-reference"),
            Ok(RuleName::DanglingReference)
        );
        assert_eq!(
            RuleName::from_str("invalid-expression"),
            Ok(RuleName::InvalidExpression)
        );
        assert_eq!(
            RuleName::from_str("run-shellcheck"),
            Ok(RuleName::RunShellcheck)
        );
    }

    #[test]
    fn rule_name_from_str_invalid() {
        RuleName::from_str("nonexistent-rule").unwrap_err();
    }
}

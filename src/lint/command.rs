use super::report::Report;
use super::sha_mismatch::ShaMismatchRule;
use super::stale_comment::StaleCommentRule;
use super::unpinned::UnpinnedRule;
use super::unsynced_manifest::UnsyncedManifestRule;
use crate::command::Command;
use crate::config::{Config, IgnoreTarget, Level, Lint as LintConfig};
use crate::domain::lock::Lock;
use crate::domain::manifest::Manifest;
use crate::domain::workflow::{Error as WorkflowError, Scanner as WorkflowScanner};
use crate::domain::workflow_actions::{
    ActionSet as WorkflowActionSet, Located as LocatedAction, WorkflowPath,
};
use crate::infra::workflow_scan::FileScanner as FileWorkflowScanner;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;

/// Canonical identifier for a lint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleName {
    ShaMismatch,
    Unpinned,
    StaleComment,
    UnsyncedManifest,
}

impl std::fmt::Display for RuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShaMismatch => write!(f, "sha-mismatch"),
            Self::Unpinned => write!(f, "unpinned"),
            Self::StaleComment => write!(f, "stale-comment"),
            Self::UnsyncedManifest => write!(f, "unsynced-manifest"),
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
            other => Err(format!("unrecognized rule name: {other}")),
        }
    }
}

/// Errors that can occur during the lint command.
#[derive(Debug, Error)]
pub enum Error {
    /// A workflow parsing or I/O error occurred.
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
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
}

impl Diagnostic {
    /// Create a new diagnostic.
    pub fn new<S: Into<String>>(rule: RuleName, level: Level, message: S) -> Self {
        Self {
            rule,
            level,
            message: message.into(),
            workflow: None,
        }
    }

    /// Set the workflow field.
    #[must_use]
    pub fn with_workflow(mut self, workflow: WorkflowPath) -> Self {
        self.workflow = Some(workflow);
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

/// Check if a diagnostic matches an ignore target using intersection semantics.
/// All specified keys in the target must match for the ignore to apply.
fn matches_ignore(
    diag: &Diagnostic,
    target: &IgnoreTarget,
    located_actions: &[LocatedAction],
) -> bool {
    // Extract action ID and workflow from the diagnostic
    let Some(diag_workflow) = &diag.workflow else {
        return false; // Can't match without workflow info
    };

    // Find the first located action from this workflow
    let diag_action = located_actions
        .iter()
        .find(|loc| loc.location.workflow == *diag_workflow)
        .map(|loc| &loc.action.id);

    // Check intersection of specified keys
    if let Some(target_action) = &target.action {
        if let Some(matched_action) = diag_action {
            if matched_action.as_str() != target_action.as_str() {
                return false;
            }
        } else {
            return false;
        }
    }

    if let Some(target_workflow) = &target.workflow
        && !diag_workflow.as_str().ends_with(target_workflow.as_str())
    {
        return false;
    }

    // Job matching would require more context from the diagnostic
    // For now, if job is specified but we can't verify it, be conservative and don't match
    if target.job.is_some() {
        return false; // Not yet implemented
    }

    true
}

/// Build a `Report` from diagnostics.
#[must_use]
pub fn format_and_report(diagnostics: Vec<Diagnostic>) -> Report {
    Report::from_diagnostics(diagnostics)
}

/// Run lint checks by scanning workflows and return diagnostics.
///
/// File-local rules (sha-mismatch, unpinned, stale-comment) run per-action during scanning.
/// Global rules (unsynced-manifest) run after the full scan completes.
///
/// # Errors
///
/// Returns [`Error::Workflow`] if a workflow parsing error occurs.
pub fn collect_diagnostics(
    manifest: &Manifest,
    lock: &Lock,
    scanner: &dyn WorkflowScanner,
    lint_config: &LintConfig,
    on_progress: &mut dyn FnMut(&str),
) -> Result<Vec<Diagnostic>, Error> {
    on_progress("Scanning workflows...");
    let sha_mismatch_level = lint_config
        .get_rule(RuleName::ShaMismatch, Level::Error)
        .level;
    let unpinned_level = lint_config.get_rule(RuleName::Unpinned, Level::Error).level;
    let stale_comment_level = lint_config
        .get_rule(RuleName::StaleComment, Level::Warn)
        .level;

    let mut all_diagnostics = Vec::new();
    let mut located = Vec::new();
    let mut action_set = WorkflowActionSet::new();

    // Phase 1: Scan workflows, running per-action rules on each action
    for result in scanner.scan() {
        let action = result?;

        // Per-action rules
        if sha_mismatch_level != Level::Off
            && let Some(mut diag) = ShaMismatchRule::check_action(&action, lock)
        {
            diag.level = sha_mismatch_level;
            if !is_ignored(
                &diag,
                RuleName::ShaMismatch,
                Level::Error,
                lint_config,
                &action,
            ) {
                all_diagnostics.push(diag);
            }
        }
        if unpinned_level != Level::Off
            && let Some(mut diag) = UnpinnedRule::check_action(&action)
        {
            diag.level = unpinned_level;
            if !is_ignored(
                &diag,
                RuleName::Unpinned,
                Level::Error,
                lint_config,
                &action,
            ) {
                all_diagnostics.push(diag);
            }
        }
        if stale_comment_level != Level::Off
            && let Some(mut diag) = StaleCommentRule::check_action(&action, lock)
        {
            diag.level = stale_comment_level;
            if !is_ignored(
                &diag,
                RuleName::StaleComment,
                Level::Warn,
                lint_config,
                &action,
            ) {
                all_diagnostics.push(diag);
            }
        }

        action_set.add(&action.action);
        located.push(action);
    }

    // Phase 2: Run global rules that need the full picture
    let unsynced_level = lint_config
        .get_rule(RuleName::UnsyncedManifest, Level::Error)
        .level;
    if unsynced_level != Level::Off {
        let ctx = Context {
            manifest,
            lock,
            workflows: &located,
            action_set: &action_set,
        };
        let rule = UnsyncedManifestRule;
        for mut diag in rule.check(&ctx) {
            diag.level = unsynced_level;
            let ignored = lint_config
                .get_rule(RuleName::UnsyncedManifest, Level::Error)
                .ignore
                .iter()
                .any(|target| matches_ignore(&diag, target, &located));
            if !ignored {
                all_diagnostics.push(diag);
            }
        }
    }

    Ok(all_diagnostics)
}

/// Check if a per-action diagnostic is ignored via lint config.
fn is_ignored(
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
    let Some(diag_workflow) = &diag.workflow else {
        return false;
    };

    if let Some(target_action) = &target.action
        && action.action.id.as_str() != target_action.as_str()
    {
        return false;
    }

    if let Some(target_workflow) = &target.workflow
        && !diag_workflow.as_str().ends_with(target_workflow.as_str())
    {
        return false;
    }

    if target.job.is_some() {
        return false;
    }

    true
}

/// The lint command struct.
pub struct Lint;

impl Command for Lint {
    type Report = Report;
    type Error = Error;

    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<Report, Error> {
        let scanner = FileWorkflowScanner::new(repo_root);

        let diagnostics = collect_diagnostics(
            &config.manifest,
            &config.lock,
            &scanner,
            &config.lint_config,
            on_progress,
        )?;

        Ok(format_and_report(diagnostics))
    }
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
    }

    #[test]
    fn rule_name_from_str_invalid() {
        RuleName::from_str("nonexistent-rule").unwrap_err();
    }
}

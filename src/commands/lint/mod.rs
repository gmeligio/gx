use crate::config::{IgnoreTarget, Level, LintConfig};
use crate::domain::{
    LocatedAction, Lock, Manifest, WorkflowActionSet, WorkflowError, WorkflowScanner,
};
use thiserror::Error;

/// Errors that can occur during the lint command
#[derive(Debug, Error)]
pub enum LintError {
    /// A workflow parsing or I/O error occurred.
    #[error(transparent)]
    Workflow(#[from] WorkflowError),

    /// Lint violations were found (errors or warnings).
    #[error("{errors} error(s) and {warnings} warning(s) found")]
    ViolationsFound { errors: usize, warnings: usize },
}

/// A single diagnostic reported by a lint rule.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Diagnostic {
    /// Name of the rule that produced this diagnostic
    pub rule: String,
    /// Severity level
    pub level: Level,
    /// Human-readable message
    pub message: String,
    /// Optional workflow file path where the issue was found
    pub workflow: Option<String>,
}

impl Diagnostic {
    /// Create a new diagnostic.
    pub fn new(rule: impl Into<String>, level: Level, message: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            level,
            message: message.into(),
            workflow: None,
        }
    }

    /// Set the workflow field.
    #[must_use]
    pub fn with_workflow(mut self, workflow: impl Into<String>) -> Self {
        self.workflow = Some(workflow.into());
        self
    }
}

/// Context shared by all lint rules during checking.
pub struct LintContext<'a> {
    /// The manifest (gx.toml)
    pub manifest: &'a Manifest,
    /// The lock file (gx.lock)
    pub lock: &'a Lock,
    /// All located actions from scanned workflows
    pub workflows: &'a [LocatedAction],
    /// Aggregated action set from all workflows
    pub action_set: &'a WorkflowActionSet,
}

/// Trait for a lint rule.
pub trait LintRule {
    /// Returns the rule's name (e.g., "sha-mismatch")
    fn name(&self) -> &str;

    /// Returns this rule's default severity level
    fn default_level(&self) -> Level;

    /// Run the lint check and return all detected diagnostics.
    /// Rules report everything they find; filtering against ignores happens in the orchestrator.
    fn check(&self, ctx: &LintContext) -> Vec<Diagnostic>;
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
        .map(|loc| &loc.id);

    // Check intersection of specified keys
    if let Some(target_action) = &target.action {
        if let Some(diag_action) = diag_action {
            if diag_action.as_str() != target_action.as_str() {
                return false;
            }
        } else {
            return false;
        }
    }

    if let Some(target_workflow) = &target.workflow
        && !diag_workflow.ends_with(target_workflow.as_str())
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

/// Format and report diagnostics to the log, returning an error if violations exist.
///
/// # Errors
///
/// Returns [`LintError::ViolationsFound`] if there are any error-level diagnostics.
pub fn format_and_report(diagnostics: &[Diagnostic]) -> Result<(), LintError> {
    use log::info;

    if diagnostics.is_empty() {
        info!("No lint issues found.");
        return Ok(());
    }

    for diag in diagnostics {
        let level_str = match diag.level {
            crate::config::Level::Error => "[error]",
            crate::config::Level::Warn => "[warn]",
            crate::config::Level::Off => "[off]",
        };
        let location = diag
            .workflow
            .as_ref()
            .map(|w| format!("{w}: "))
            .unwrap_or_default();
        info!("{} {}{}: {}", level_str, location, diag.rule, diag.message);
    }

    let error_count = diagnostics
        .iter()
        .filter(|d| d.level == crate::config::Level::Error)
        .count();
    let warn_count = diagnostics
        .iter()
        .filter(|d| d.level == crate::config::Level::Warn)
        .count();
    info!(
        "{} issue(s) ({} error{}, {} warning{})",
        diagnostics.len(),
        error_count,
        if error_count == 1 { "" } else { "s" },
        warn_count,
        if warn_count == 1 { "" } else { "s" }
    );

    if error_count > 0 {
        return Err(LintError::ViolationsFound {
            errors: error_count,
            warnings: warn_count,
        });
    }

    Ok(())
}

/// Run lint checks by scanning workflows and return diagnostics.
///
/// File-local rules (sha-mismatch, unpinned, stale-comment) run per-action during scanning.
/// Global rules (unsynced-manifest) run after the full scan completes.
///
/// # Errors
///
/// Returns [`LintError::Workflow`] if a workflow parsing error occurs.
pub fn run(
    manifest: &Manifest,
    lock: &Lock,
    scanner: &dyn WorkflowScanner,
    lint_config: &LintConfig,
) -> Result<Vec<Diagnostic>, LintError> {
    let sha_mismatch_level = lint_config.get_rule("sha-mismatch", Level::Error).level;
    let unpinned_level = lint_config.get_rule("unpinned", Level::Error).level;
    let stale_comment_level = lint_config.get_rule("stale-comment", Level::Warn).level;

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
            if !is_ignored(&diag, "sha-mismatch", Level::Error, lint_config, &action) {
                all_diagnostics.push(diag);
            }
        }
        if unpinned_level != Level::Off
            && let Some(mut diag) = UnpinnedRule::check_action(&action)
        {
            diag.level = unpinned_level;
            if !is_ignored(&diag, "unpinned", Level::Error, lint_config, &action) {
                all_diagnostics.push(diag);
            }
        }
        if stale_comment_level != Level::Off
            && let Some(mut diag) = StaleCommentRule::check_action(&action, lock)
        {
            diag.level = stale_comment_level;
            if !is_ignored(&diag, "stale-comment", Level::Warn, lint_config, &action) {
                all_diagnostics.push(diag);
            }
        }

        action_set.add_located(&action);
        located.push(action);
    }

    // Phase 2: Run global rules that need the full picture
    let unsynced_level = lint_config
        .get_rule("unsynced-manifest", Level::Error)
        .level;
    if unsynced_level != Level::Off {
        let ctx = LintContext {
            manifest,
            lock,
            workflows: &located,
            action_set: &action_set,
        };
        let rule = UnsyncedManifestRule;
        for mut diag in rule.check(&ctx) {
            diag.level = unsynced_level;
            let ignored = lint_config
                .get_rule("unsynced-manifest", Level::Error)
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
    rule_name: &str,
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
        && action.id.as_str() != target_action.as_str()
    {
        return false;
    }

    if let Some(target_workflow) = &target.workflow
        && !diag_workflow.ends_with(target_workflow.as_str())
    {
        return false;
    }

    if target.job.is_some() {
        return false;
    }

    true
}

mod sha_mismatch;
mod stale_comment;
mod unpinned;
mod unsynced_manifest;

use sha_mismatch::ShaMismatchRule;
use stale_comment::StaleCommentRule;
use unpinned::UnpinnedRule;
use unsynced_manifest::UnsyncedManifestRule;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_can_be_created() {
        let diag = Diagnostic::new("test-rule", Level::Error, "test message");
        assert_eq!(diag.rule, "test-rule");
        assert_eq!(diag.level, Level::Error);
        assert_eq!(diag.message, "test message");
        assert!(diag.workflow.is_none());
    }

    #[test]
    fn diagnostic_with_workflow() {
        let diag = Diagnostic::new("test-rule", Level::Warn, "test")
            .with_workflow(".github/workflows/ci.yml");
        assert_eq!(diag.workflow, Some(".github/workflows/ci.yml".to_string()));
    }
}

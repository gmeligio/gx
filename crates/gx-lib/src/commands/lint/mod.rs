use crate::config::{IgnoreTarget, Level, LintConfig};
use crate::domain::{LocatedAction, Lock, Manifest, WorkflowActionSet};
use crate::infrastructure::WorkflowError;
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

/// Run lint checks on the given manifest/lock/workflows and return diagnostics.
///
/// # Errors
///
/// Returns [`LintError::Workflow`] if a workflow parsing error occurs.
/// Returns [`LintError::ViolationsFound`] if lint diagnostics are produced (either errors or warnings).
pub fn run(
    manifest: &Manifest,
    lock: &Lock,
    workflows: &[LocatedAction],
    action_set: &WorkflowActionSet,
    lint_config: &LintConfig,
) -> Result<Vec<Diagnostic>, LintError> {
    let ctx = LintContext {
        manifest,
        lock,
        workflows,
        action_set,
    };

    let mut all_diagnostics = Vec::new();

    // Build a vec of rules with their configured levels
    let rules: Vec<(Box<dyn LintRule>, Level)> = vec![
        (
            Box::new(ShaMismatchRule),
            lint_config.get_rule("sha-mismatch", Level::Error).level,
        ),
        (
            Box::new(UnpinnedRule),
            lint_config.get_rule("unpinned", Level::Error).level,
        ),
        (
            Box::new(UnsyncedManifestRule),
            lint_config
                .get_rule("unsynced-manifest", Level::Error)
                .level,
        ),
        (
            Box::new(StaleCommentRule),
            lint_config.get_rule("stale-comment", Level::Warn).level,
        ),
    ];

    // Run each rule and collect diagnostics
    for (rule, configured_level) in rules {
        if configured_level == Level::Off {
            continue;
        }

        let rule_diagnostics = rule.check(&ctx);
        for mut diag in rule_diagnostics {
            // Apply configured level (may be different from rule default)
            diag.level = configured_level;

            // Filter against ignores
            let ignored = lint_config
                .get_rule(rule.name(), rule.default_level())
                .ignore
                .iter()
                .any(|target| matches_ignore(&diag, target, workflows));

            if !ignored {
                all_diagnostics.push(diag);
            }
        }
    }

    Ok(all_diagnostics)
}

// Rule implementations
mod sha_mismatch;
mod stale_comment;
mod unpinned;
mod unsynced_manifest;

pub use sha_mismatch::ShaMismatchRule;
pub use stale_comment::StaleCommentRule;
pub use unpinned::UnpinnedRule;
pub use unsynced_manifest::UnsyncedManifestRule;

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

use super::{Context, Diagnostic, Rule, RuleName};
use crate::config::Level;
use crate::domain::action::spec::Spec;
use crate::domain::action::specifier::Specifier;

/// sha-mismatch rule: detects when a workflow SHA doesn't match the lock file.
pub struct ShaMismatchRule;

impl ShaMismatchRule {
    /// Check a single action for the sha-mismatch rule.
    pub fn check_action(
        action: &crate::domain::workflow_actions::Located,
        lock: &crate::domain::lock::Lock,
    ) -> Option<Diagnostic> {
        if !action.action.version.is_sha() {
            return None;
        }

        let key = Spec::new(
            action.action.id.clone(),
            Specifier::from_v1(action.action.version.as_str()),
        );
        if lock.has(&key) {
            return None;
        }

        let msg = format!(
            "{}: action {} SHA {} not found in lock file",
            &action.location.workflow,
            &action.action.id,
            action.action.version.as_str()
        );
        Some(
            Diagnostic::new(RuleName::ShaMismatch, Level::Error, msg)
                .with_workflow(action.location.workflow.clone()),
        )
    }
}

impl Rule for ShaMismatchRule {
    fn name(&self) -> RuleName {
        RuleName::ShaMismatch
    }

    fn default_level(&self) -> Level {
        Level::Error
    }

    fn check(&self, ctx: &Context) -> Vec<Diagnostic> {
        ctx.workflows
            .iter()
            .filter_map(|a| Self::check_action(a, ctx.lock))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Level, Rule as _, RuleName, ShaMismatchRule};

    #[test]
    fn sha_mismatch_rule_has_correct_metadata() {
        let rule = ShaMismatchRule;
        assert_eq!(rule.name(), RuleName::ShaMismatch);
        assert_eq!(rule.default_level(), Level::Error);
    }
}

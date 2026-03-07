use super::{Diagnostic, LintContext, LintRule};
use crate::config::Level;
use crate::domain::LockKey;

/// sha-mismatch rule: detects when a workflow SHA doesn't match the lock file.
pub struct ShaMismatchRule;

impl ShaMismatchRule {
    /// Check a single action for the sha-mismatch rule.
    pub fn check_action(
        action: &crate::domain::LocatedAction,
        lock: &crate::domain::Lock,
    ) -> Option<Diagnostic> {
        if !action.version.is_sha() {
            return None;
        }

        let key = LockKey::new(action.id.clone(), action.version.clone());
        if lock.has(&key) {
            return None;
        }

        let msg = format!(
            "{}: action {} SHA {} not found in lock file",
            &action.location.workflow,
            &action.id,
            action.version.as_str()
        );
        Some(
            Diagnostic::new("sha-mismatch", Level::Error, msg)
                .with_workflow(&action.location.workflow),
        )
    }
}

impl LintRule for ShaMismatchRule {
    fn name(&self) -> &'static str {
        "sha-mismatch"
    }

    fn default_level(&self) -> Level {
        Level::Error
    }

    fn check(&self, ctx: &LintContext) -> Vec<Diagnostic> {
        ctx.workflows
            .iter()
            .filter_map(|a| Self::check_action(a, ctx.lock))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha_mismatch_rule_has_correct_metadata() {
        let rule = ShaMismatchRule;
        assert_eq!(rule.name(), "sha-mismatch");
        assert_eq!(rule.default_level(), Level::Error);
    }
}

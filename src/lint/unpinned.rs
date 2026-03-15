use super::{Context, Diagnostic, Rule};
use crate::config::Level;

/// unpinned rule: detects actions that use tag refs instead of SHA pins.
pub struct UnpinnedRule;

impl UnpinnedRule {
    /// Check a single action for the unpinned rule.
    pub fn check_action(action: &crate::domain::workflow_actions::Located) -> Option<Diagnostic> {
        if action.action.version.is_sha() {
            return None;
        }
        let msg = format!(
            "{}: action {} uses tag reference {} instead of SHA pin",
            &action.location.workflow,
            &action.action.id,
            action.action.version.as_str()
        );
        Some(
            Diagnostic::new("unpinned", Level::Error, msg).with_workflow(&action.location.workflow),
        )
    }
}

impl Rule for UnpinnedRule {
    fn name(&self) -> &'static str {
        "unpinned"
    }

    fn default_level(&self) -> Level {
        Level::Error
    }

    fn check(&self, ctx: &Context) -> Vec<Diagnostic> {
        ctx.workflows
            .iter()
            .filter_map(Self::check_action)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Level, Rule as _, UnpinnedRule};

    #[test]
    fn unpinned_rule_has_correct_metadata() {
        let rule = UnpinnedRule;
        assert_eq!(rule.name(), "unpinned");
        assert_eq!(rule.default_level(), Level::Error);
    }
}

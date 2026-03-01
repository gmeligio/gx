use super::{Diagnostic, LintContext, LintRule};
use crate::config::Level;

/// unpinned rule: detects actions that use tag refs instead of SHA pins.
pub struct UnpinnedRule;

impl LintRule for UnpinnedRule {
    fn name(&self) -> &'static str {
        "unpinned"
    }

    fn default_level(&self) -> Level {
        Level::Error
    }

    fn check(&self, ctx: &LintContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for located in ctx.workflows {
            // If the version is not a SHA, it's unpinned (e.g., @v4, @main, etc.)
            if !located.version.is_sha() {
                let msg = format!(
                    "{}: action {} uses tag reference {} instead of SHA pin",
                    &located.location.workflow,
                    &located.id,
                    located.version.as_str()
                );

                diagnostics.push(
                    Diagnostic::new(self.name(), self.default_level(), msg)
                        .with_workflow(&located.location.workflow),
                );
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpinned_rule_has_correct_metadata() {
        let rule = UnpinnedRule;
        assert_eq!(rule.name(), "unpinned");
        assert_eq!(rule.default_level(), Level::Error);
    }
}

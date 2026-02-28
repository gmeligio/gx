use super::{Diagnostic, LintContext, LintRule};
use crate::config::Level;

/// stale-comment rule: detects when a version comment doesn't match the lock file.
pub struct StaleCommentRule;

impl LintRule for StaleCommentRule {
    fn name(&self) -> &'static str {
        "stale-comment"
    }

    fn default_level(&self) -> Level {
        Level::Warn
    }

    fn check(&self, _ctx: &LintContext) -> Vec<Diagnostic> {
        // The stale-comment rule requires extracting version comments from workflow files,
        // which is complex and requires YAML comment extraction. For now, we'll keep this
        // rule as a no-op stub that returns no diagnostics.
        // Future enhancement: parse comments from workflow files and compare against lock.
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_comment_rule_has_correct_metadata() {
        let rule = StaleCommentRule;
        assert_eq!(rule.name(), "stale-comment");
        assert_eq!(rule.default_level(), Level::Warn);
    }
}

use super::{Context, Diagnostic, Rule, RuleName};
use crate::config::Level;

/// unpinned rule: detects actions that use tag refs instead of SHA pins.
pub struct UnpinnedRule;

impl UnpinnedRule {
    /// Check a single action for the unpinned rule.
    ///
    /// An action is considered pinned when the `uses:` ref is a 40-char commit
    /// SHA. That can present in two shapes after parsing:
    ///
    /// - `uses: owner/repo@<sha>` — the SHA lands in `version` (no comment).
    /// - `uses: owner/repo@<sha> # vX.Y.Z` — the SHA lands in `sha`, and
    ///   `version` holds the human-readable tag from the comment.
    ///
    /// Both shapes are valid pins, so we accept either.
    pub fn check_action(action: &crate::domain::workflow_actions::Located) -> Option<Diagnostic> {
        if action.action.sha.is_some() || action.action.version.is_sha() {
            return None;
        }
        let msg = format!(
            "{}: action {} uses tag reference {} instead of SHA pin",
            &action.location.workflow,
            &action.action.id,
            action.action.version.as_str()
        );
        Some(
            Diagnostic::new(RuleName::Unpinned, Level::Error, msg)
                .with_workflow(action.location.workflow.clone()),
        )
    }
}

impl Rule for UnpinnedRule {
    fn name(&self) -> RuleName {
        RuleName::Unpinned
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
    use super::{Level, Rule as _, RuleName, UnpinnedRule};
    use crate::domain::action::identity::{ActionId, CommitSha, Version};
    use crate::domain::workflow_actions::{Located, Location, WorkflowAction, WorkflowPath};

    const VALID_SHA: &str = "8e8c483db84b4bee98b60c0593521ed34d9990e8";

    fn located(version: &str, sha: Option<&str>) -> Located {
        Located {
            action: WorkflowAction {
                id: ActionId::from("actions/checkout"),
                version: Version::from(version),
                sha: sha.map(CommitSha::from),
            },
            location: Location {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
                job: None,
                step: None,
            },
        }
    }

    #[test]
    fn unpinned_rule_has_correct_metadata() {
        let rule = UnpinnedRule;
        assert_eq!(rule.name(), RuleName::Unpinned);
        assert_eq!(rule.default_level(), Level::Error);
    }

    #[test]
    fn sha_pin_with_version_comment_is_not_flagged() {
        let action = located("v6.0.1", Some(VALID_SHA));
        assert!(UnpinnedRule::check_action(&action).is_none());
    }

    #[test]
    fn sha_pin_without_comment_is_not_flagged() {
        let action = located(VALID_SHA, None);
        assert!(UnpinnedRule::check_action(&action).is_none());
    }

    #[test]
    fn tag_reference_is_flagged() {
        let action = located("v4", None);
        assert!(UnpinnedRule::check_action(&action).is_some());
    }
}

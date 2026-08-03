use super::{Context, Diagnostic, Rule, RuleName};
use crate::config::Level;

/// unpinned rule: detects actions that use tag refs instead of SHA pins.
pub struct UnpinnedRule;

impl UnpinnedRule {
    /// Check a single action for the unpinned rule.
    ///
    /// An action is pinned when the `uses:` ref carries a commit SHA — either a
    /// bare `@<sha>` or a `@<sha> # vX.Y.Z` pin. The typed reference answers this
    /// directly: `pin_sha()` is `Some` for both pinned shapes and `None` for a
    /// bare tag/branch ref.
    pub fn check_action(action: &crate::domain::file::actions::Located) -> Option<Diagnostic> {
        if action.action.reference.pin_sha().is_some() {
            return None;
        }
        let msg = format!(
            "action {} uses tag reference {} instead of SHA pin",
            &action.action.id,
            action.action.reference.label()
        );
        Some(
            Diagnostic::new(RuleName::Unpinned, Level::Error, msg)
                .with_workflow(action.location.workflow.clone())
                .with_line(action.location.line),
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
#[expect(clippy::unwrap_used, reason = "tests use unwrap freely")]
mod tests {
    use super::{Level, Rule as _, RuleName, UnpinnedRule};
    use crate::domain::action::identity::{ActionId, CommitSha, Version};
    use crate::domain::action::uses_ref::ParsedRef;
    use crate::domain::file::actions::{Located, Location, WorkflowAction};
    use crate::domain::file::site::WorkflowPath;

    const VALID_SHA: &str = "8e8c483db84b4bee98b60c0593521ed34d9990e8";

    fn located(version: &str, sha: Option<&str>) -> Located {
        located_at(version, sha, None)
    }

    /// Mirror `UsesRef::interpret`: `Some(sha)` → a `Pinned` pin; a bare 40-hex
    /// `version` → `Sha`; anything else → a plain `Ref`.
    fn located_at(version: &str, sha: Option<&str>, line: Option<u32>) -> Located {
        let reference = match sha {
            Some(sha_str) => ParsedRef::Pinned {
                sha: CommitSha::from(sha_str),
                comment: Version::from(version),
            },
            None if CommitSha::is_valid(version) => ParsedRef::Sha(CommitSha::from(version)),
            None => ParsedRef::Ref(Version::from(version)),
        };
        Located {
            action: WorkflowAction {
                id: ActionId::from("actions/checkout"),
                reference,
            },
            location: Location {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
                job: None,
                step: None,
                line,
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

    #[test]
    fn diagnostic_carries_source_line_when_known() {
        let action = located_at("v4", None, Some(12));
        let diag = UnpinnedRule::check_action(&action).unwrap();
        assert_eq!(diag.line, Some(12));
    }

    #[test]
    fn diagnostic_omits_line_when_unknown() {
        let action = located_at("v4", None, None);
        let diag = UnpinnedRule::check_action(&action).unwrap();
        assert_eq!(diag.line, None);
    }

    #[test]
    fn message_does_not_embed_workflow_path() {
        // The renderer prepends the location; the message must not repeat it.
        let action = located("v4", None);
        let diag = UnpinnedRule::check_action(&action).unwrap();
        assert!(
            !diag.message.contains(".github/workflows/ci.yml"),
            "message should not embed the workflow path: {}",
            diag.message
        );
    }
}

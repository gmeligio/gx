use super::{Context, Diagnostic, Rule, RuleName};
use crate::config::Level;
use crate::domain::action::spec::Spec;
use crate::domain::action::specifier::Specifier;
use crate::domain::action::uses_ref::ParsedRef;

/// stale-comment rule: detects when a version comment doesn't match the lock file.
pub struct StaleCommentRule;

impl StaleCommentRule {
    /// Check a single action for the stale-comment rule.
    ///
    /// Only a `# comment` pin (`uses: owner/repo@<sha> # v4`) can carry a stale
    /// comment: it pairs a pinned SHA with a human-readable version to reconcile.
    pub fn check_action(
        action: &crate::domain::workflow_actions::Located,
        lock: &crate::domain::lock::Lock,
    ) -> Option<Diagnostic> {
        let ParsedRef::Pinned { sha, comment } = &action.action.reference else {
            return None;
        };

        let key = Spec::new(
            action.action.id.clone(),
            Specifier::from_v1(comment.as_str()),
        );
        let entry = lock.get(&key)?;

        if entry.commit.sha == *sha {
            return None;
        }

        let msg = format!(
            "action {} version {} has stale comment (SHA {} does not match lock SHA {})",
            &action.action.id,
            comment.as_str(),
            sha.as_str(),
            entry.commit.sha.as_str()
        );
        Some(
            Diagnostic::new(RuleName::StaleComment, Level::Warn, msg)
                .with_workflow(action.location.workflow.clone())
                .with_line(action.location.line),
        )
    }
}

impl Rule for StaleCommentRule {
    fn name(&self) -> RuleName {
        RuleName::StaleComment
    }

    fn default_level(&self) -> Level {
        Level::Warn
    }

    fn check(&self, ctx: &Context) -> Vec<Diagnostic> {
        ctx.workflows
            .iter()
            .filter_map(|a| Self::check_action(a, ctx.lock))
            .collect()
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Level, Rule as _, RuleName, StaleCommentRule};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Version};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::lock::Lock;
    use crate::domain::manifest::Manifest;
    use crate::domain::workflow_actions::{ActionSet, Located, Location, WorkflowPath};

    fn make_lock(action: &str, version: &str, sha: &str) -> Lock {
        let mut lock = Lock::default();
        lock.set(
            &Spec::new(ActionId::from(action), Specifier::from_v1(version)),
            ResolvedRef::Tag(Version::from(version)),
            Commit {
                sha: CommitSha::from(sha),
                repository: ActionId::from(action).base_repo(),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
        lock
    }

    fn make_located(action: &str, version: &str, sha: Option<&str>, workflow: &str) -> Located {
        use crate::domain::action::uses_ref::ParsedRef;
        use crate::domain::workflow_actions::WorkflowAction;
        // Mirror `UsesRef::interpret`: a `# comment` pin → `Pinned`; a bare
        // 40-hex ref → `Sha`; otherwise a plain tag/branch `Ref`.
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
                id: ActionId::from(action),
                reference,
            },
            location: Location {
                workflow: WorkflowPath::new(workflow),
                job: None,
                step: None,
                line: None,
            },
        }
    }

    #[test]
    fn stale_comment_rule_has_correct_metadata() {
        let rule = StaleCommentRule;
        assert_eq!(rule.name(), RuleName::StaleComment);
        assert_eq!(rule.default_level(), Level::Warn);
    }

    #[test]
    fn stale_comment_matches_lock_sha_produces_no_diagnostic() {
        let rule = StaleCommentRule;
        let lock = make_lock("actions/checkout", "v4", "abc123def456");
        let workflows = vec![make_located(
            "actions/checkout",
            "v4",
            Some("abc123def456"),
            ".github/workflows/ci.yml",
        )];
        let manifest = Manifest::default();
        let action_set = ActionSet::new();

        let ctx = crate::lint::Context {
            manifest: &manifest,
            lock: &lock,
            workflows: &workflows,
            workflows_full: &[],
            action_set: &action_set,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(
            diagnostics.len(),
            0,
            "Matching SHA should produce no diagnostic"
        );
    }

    #[test]
    fn stale_comment_does_not_match_lock_sha_produces_warn_diagnostic() {
        let rule = StaleCommentRule;
        let lock = make_lock("actions/checkout", "v4", "def456789012");
        let workflows = vec![make_located(
            "actions/checkout",
            "v4",
            Some("abc123def456"),
            ".github/workflows/ci.yml",
        )];
        let manifest = Manifest::default();
        let action_set = ActionSet::new();

        let ctx = crate::lint::Context {
            manifest: &manifest,
            lock: &lock,
            workflows: &workflows,
            workflows_full: &[],
            action_set: &action_set,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(
            diagnostics.len(),
            1,
            "Mismatched SHA should produce one diagnostic"
        );
        assert_eq!(diagnostics[0].level, Level::Warn);
        assert_eq!(diagnostics[0].rule, RuleName::StaleComment);
        assert!(diagnostics[0].message.contains("stale comment"));
    }

    #[test]
    fn stale_comment_action_without_comment_is_skipped() {
        let rule = StaleCommentRule;
        let lock = make_lock("actions/checkout", "v4", "abc123def456");
        let workflows = vec![make_located(
            "actions/checkout",
            "v4",
            None, // No SHA comment
            ".github/workflows/ci.yml",
        )];
        let manifest = Manifest::default();
        let action_set = ActionSet::new();

        let ctx = crate::lint::Context {
            manifest: &manifest,
            lock: &lock,
            workflows: &workflows,
            workflows_full: &[],
            action_set: &action_set,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(
            diagnostics.len(),
            0,
            "Action without comment should be skipped"
        );
    }
}

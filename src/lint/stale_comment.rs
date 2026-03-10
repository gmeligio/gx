use super::{Diagnostic, LintContext, LintRule};
use crate::config::Level;
use crate::domain::{LockKey, Specifier};

/// stale-comment rule: detects when a version comment doesn't match the lock file.
pub struct StaleCommentRule;

impl StaleCommentRule {
    /// Check a single action for the stale-comment rule.
    pub fn check_action(
        action: &crate::domain::LocatedAction,
        lock: &crate::domain::Lock,
    ) -> Option<Diagnostic> {
        let sha = action.sha.as_ref()?;

        let key = LockKey::new(
            action.id.clone(),
            Specifier::from_v1(action.version.as_str()),
        );
        let lock_entry = lock.get(&key)?;

        if lock_entry.sha == *sha {
            return None;
        }

        let msg = format!(
            "{}: action {} version {} has stale comment (SHA {} does not match lock SHA {})",
            &action.location.workflow,
            &action.id,
            action.version.as_str(),
            sha.as_str(),
            lock_entry.sha.as_str()
        );
        Some(
            Diagnostic::new("stale-comment", Level::Warn, msg)
                .with_workflow(&action.location.workflow),
        )
    }
}

impl LintRule for StaleCommentRule {
    fn name(&self) -> &'static str {
        "stale-comment"
    }

    fn default_level(&self) -> Level {
        Level::Warn
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
    use super::{Level, LintRule, StaleCommentRule};
    use crate::domain::{
        ActionId, CommitSha, LocatedAction, Lock, Manifest, ResolvedAction, Specifier, Version,
        WorkflowActionSet, WorkflowLocation,
    };

    fn make_lock(action: &str, version: &str, sha: &str) -> Lock {
        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from(action),
            Specifier::from_v1(version),
            CommitSha::from(sha),
            ActionId::from(action).base_repo(),
            Some(crate::domain::RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        ));
        lock
    }

    fn make_located(
        action: &str,
        version: &str,
        sha: Option<&str>,
        workflow: &str,
    ) -> LocatedAction {
        LocatedAction {
            id: ActionId::from(action),
            version: Version::from(version),
            sha: sha.map(CommitSha::from),
            location: WorkflowLocation {
                workflow: workflow.to_string(),
                job: None,
                step: None,
            },
        }
    }

    #[test]
    fn stale_comment_rule_has_correct_metadata() {
        let rule = StaleCommentRule;
        assert_eq!(rule.name(), "stale-comment");
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
        let action_set = WorkflowActionSet::new();

        let ctx = crate::lint::LintContext {
            manifest: &manifest,
            lock: &lock,
            workflows: &workflows,
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
        let action_set = WorkflowActionSet::new();

        let ctx = crate::lint::LintContext {
            manifest: &manifest,
            lock: &lock,
            workflows: &workflows,
            action_set: &action_set,
        };

        let diagnostics = rule.check(&ctx);
        assert_eq!(
            diagnostics.len(),
            1,
            "Mismatched SHA should produce one diagnostic"
        );
        assert_eq!(diagnostics[0].level, Level::Warn);
        assert_eq!(diagnostics[0].rule, "stale-comment");
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
        let action_set = WorkflowActionSet::new();

        let ctx = crate::lint::LintContext {
            manifest: &manifest,
            lock: &lock,
            workflows: &workflows,
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

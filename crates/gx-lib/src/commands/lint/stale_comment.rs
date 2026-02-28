use super::{Diagnostic, LintContext, LintRule};
use crate::config::Level;
use crate::domain::LockKey;

/// stale-comment rule: detects when a version comment doesn't match the lock file.
pub struct StaleCommentRule;

impl LintRule for StaleCommentRule {
    fn name(&self) -> &'static str {
        "stale-comment"
    }

    fn default_level(&self) -> Level {
        Level::Warn
    }

    fn check(&self, ctx: &LintContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for located in ctx.workflows {
            // Only check actions that have a version comment (sha is Some)
            let Some(sha) = &located.sha else {
                continue; // No comment to validate
            };

            // Look up what version this SHA should map to in the lock
            let key = LockKey::new(located.id.clone(), located.version.clone());
            let Some(lock_sha) = ctx.lock.get(&key) else {
                // Lock doesn't have an entry for this action@version
                // This is a different issue (unsynced manifest or missing lock entry)
                continue;
            };

            // Compare the SHA from the comment against what the lock says
            if lock_sha != sha {
                let msg = format!(
                    "{}: action {} version {} has stale comment (SHA {} does not match lock SHA {})",
                    &located.location.workflow,
                    &located.id,
                    located.version.as_str(),
                    sha.as_str(),
                    lock_sha.as_str()
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
    use crate::domain::{
        ActionId, CommitSha, LocatedAction, Lock, Manifest, ResolvedAction, Version,
        WorkflowActionSet, WorkflowLocation,
    };

    fn make_lock(action: &str, version: &str, sha: &str) -> Lock {
        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from(action),
            Version::from(version),
            CommitSha::from(sha),
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

        let ctx = super::super::LintContext {
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

        let ctx = super::super::LintContext {
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

        let ctx = super::super::LintContext {
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

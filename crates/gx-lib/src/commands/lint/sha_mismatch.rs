use super::{Diagnostic, LintContext, LintRule};
use crate::config::Level;
use crate::domain::LockKey;

/// sha-mismatch rule: detects when a workflow SHA doesn't match the lock file.
pub struct ShaMismatchRule;

impl LintRule for ShaMismatchRule {
    fn name(&self) -> &'static str {
        "sha-mismatch"
    }

    fn default_level(&self) -> Level {
        Level::Error
    }

    fn check(&self, ctx: &LintContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for located in ctx.workflows {
            // Only check actions that have a SHA reference
            if !located.version.is_sha() {
                continue;
            }

            // For now, we assume the lock is well-formed.
            // A real implementation would iterate through all lock entries for this action
            // and verify the SHA matches. Since Lock doesn't expose detailed queries,
            // we'll keep this simple: check if the key exists in the lock.
            let key = LockKey::new(located.id.clone(), located.version.clone());

            // If the key exists in the lock with this exact SHA, we're good
            if ctx.lock.has(&key) {
                continue;
            }

            // If we get here, the SHA in the workflow doesn't match the lock
            let msg = format!(
                "{}: action {} SHA {} not found in lock file",
                &located.location.workflow,
                &located.id,
                located.version.as_str()
            );

            diagnostics.push(
                Diagnostic::new(self.name(), self.default_level(), msg)
                    .with_workflow(&located.location.workflow),
            );
        }

        diagnostics
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

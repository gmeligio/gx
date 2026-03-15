use super::{Context, Diagnostic, Rule, RuleName};
use crate::config::Level;
use std::collections::HashSet;

/// unsynced-manifest rule: detects when manifest and workflows have different action sets.
pub struct UnsyncedManifestRule;

impl Rule for UnsyncedManifestRule {
    fn name(&self) -> RuleName {
        RuleName::UnsyncedManifest
    }

    fn default_level(&self) -> Level {
        Level::Error
    }

    fn check(&self, ctx: &Context) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let workflow_actions: HashSet<_> = ctx.action_set.action_ids().cloned().collect();
        let manifest_actions: HashSet<_> = ctx.manifest.specs().map(|s| s.id.clone()).collect();

        // Actions in workflows but not in manifest
        for action_id in workflow_actions.difference(&manifest_actions) {
            let msg = format!(
                "action {action_id} is used in workflows but not declared in manifest (gx.toml)"
            );
            diagnostics.push(Diagnostic::new(
                RuleName::UnsyncedManifest,
                self.default_level(),
                msg,
            ));
        }

        // Actions in manifest but not in any workflow
        for action_id in manifest_actions.difference(&workflow_actions) {
            let msg = format!(
                "action {action_id} is declared in manifest (gx.toml) but not used in any workflow"
            );
            diagnostics.push(Diagnostic::new(
                RuleName::UnsyncedManifest,
                self.default_level(),
                msg,
            ));
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::{Level, Rule as _, RuleName, UnsyncedManifestRule};

    #[test]
    fn unsynced_manifest_rule_has_correct_metadata() {
        let rule = UnsyncedManifestRule;
        assert_eq!(rule.name(), RuleName::UnsyncedManifest);
        assert_eq!(rule.default_level(), Level::Error);
    }
}

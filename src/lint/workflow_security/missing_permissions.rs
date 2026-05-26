use crate::config::Level;
use crate::domain::workflow_parsed::Parsed;
use crate::lint::{Context, Diagnostic, Rule, RuleName};

/// `missing-permissions` rule: flags workflows that omit a top-level `permissions:` block.
///
/// Without an explicit block, the workflow inherits the org/repo default token scope,
/// which is broader than `contents: read` in many setups. Making the block mandatory turns
/// "what scope does this workflow run with?" into a PR-review question instead of an
/// org-policy investigation.
pub struct MissingPermissionsRule;

impl MissingPermissionsRule {
    /// Check a single parsed workflow.
    pub fn check_workflow(workflow: &Parsed) -> Option<Diagnostic> {
        if workflow.permissions.is_some() {
            return None;
        }
        let msg = format!(
            "{}: workflow has no top-level `permissions:` block — declare one (typically `permissions: {{ contents: read }}`) to make the token scope explicit",
            workflow.path
        );
        Some(
            Diagnostic::new(RuleName::MissingPermissions, Level::Error, msg)
                .with_workflow(workflow.path.clone()),
        )
    }
}

impl Rule for MissingPermissionsRule {
    fn name(&self) -> RuleName {
        RuleName::MissingPermissions
    }

    fn default_level(&self) -> Level {
        Level::Error
    }

    fn check(&self, ctx: &Context) -> Vec<Diagnostic> {
        ctx.workflows_full
            .iter()
            .filter_map(Self::check_workflow)
            .collect()
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap and indexing freely"
)]
mod tests {
    use super::*;
    use crate::domain::workflow_actions::WorkflowPath;

    fn parse(content: &str) -> Parsed {
        Parsed::from_yaml(WorkflowPath::new(".github/workflows/ci.yml"), content).unwrap()
    }

    #[test]
    fn rule_metadata() {
        let r = MissingPermissionsRule;
        assert_eq!(r.name(), RuleName::MissingPermissions);
        assert_eq!(r.default_level(), Level::Error);
    }

    #[test]
    fn workflow_with_top_level_permissions_is_clean() {
        let p = parse("on: push\npermissions:\n  contents: read\njobs: {}\n");
        assert!(MissingPermissionsRule::check_workflow(&p).is_none());
    }

    #[test]
    fn workflow_without_permissions_fails() {
        let p = parse("on: push\njobs: {}\n");
        let diag = MissingPermissionsRule::check_workflow(&p).unwrap();
        assert_eq!(diag.rule, RuleName::MissingPermissions);
        assert_eq!(diag.level, Level::Error);
        assert!(diag.workflow.is_some());
    }
}

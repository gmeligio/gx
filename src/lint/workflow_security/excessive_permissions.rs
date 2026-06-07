use crate::config::Level;
use crate::domain::workflow_parsed::{Parsed, Permissions};
use crate::lint::{Context, Diagnostic, Rule, RuleName};

/// `excessive-permissions` rule: flags when top-level `permissions:` declares anything
/// broader than `contents: read`. Broader scopes belong at job level so they narrow to
/// the specific job that needs them.
pub struct ExcessivePermissionsRule;

impl ExcessivePermissionsRule {
    /// Returns a diagnostic when the workflow's top-level `permissions:` are excessive.
    pub fn check_workflow(workflow: &Parsed) -> Option<Diagnostic> {
        let perms = workflow.permissions.as_ref()?;
        if !perms.is_excessive() {
            return None;
        }
        let scope = match perms {
            Permissions::WriteAll => "write-all",
            Permissions::ReadAll => "read-all (broader than `contents: read`)",
            Permissions::Specific(_) => "a write-capable scope",
            Permissions::Empty => return None,
        };
        let msg = format!(
            "top-level `permissions:` declares {scope} — move broader scopes to the specific job that needs them"
        );
        Some(
            Diagnostic::new(RuleName::ExcessivePermissions, Level::Error, msg)
                .with_workflow(workflow.path.clone()),
        )
    }
}

impl Rule for ExcessivePermissionsRule {
    fn name(&self) -> RuleName {
        RuleName::ExcessivePermissions
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
#[expect(clippy::unwrap_used, reason = "tests use unwrap freely")]
mod tests {
    use super::*;
    use crate::domain::workflow_actions::WorkflowPath;

    fn parse(content: &str) -> Parsed {
        Parsed::from_yaml(WorkflowPath::new(".github/workflows/ci.yml"), content).unwrap()
    }

    #[test]
    fn rule_metadata() {
        let r = ExcessivePermissionsRule;
        assert_eq!(r.name(), RuleName::ExcessivePermissions);
        assert_eq!(r.default_level(), Level::Error);
    }

    #[test]
    fn contents_read_only_is_clean() {
        let p = parse("on: push\npermissions:\n  contents: read\njobs: {}\n");
        assert!(ExcessivePermissionsRule::check_workflow(&p).is_none());
    }

    #[test]
    fn write_all_errors() {
        let p = parse("on: push\npermissions: write-all\njobs: {}\n");
        let d = ExcessivePermissionsRule::check_workflow(&p).unwrap();
        assert_eq!(d.level, Level::Error);
        assert!(d.message.contains("write-all"));
    }

    #[test]
    fn read_all_errors_because_broader_than_contents_read() {
        let p = parse("on: push\npermissions: read-all\njobs: {}\n");
        let d = ExcessivePermissionsRule::check_workflow(&p).unwrap();
        assert_eq!(d.level, Level::Error);
    }

    #[test]
    fn contents_write_at_top_errors() {
        let p = parse("on: push\npermissions:\n  contents: write\njobs: {}\n");
        let d = ExcessivePermissionsRule::check_workflow(&p).unwrap();
        assert_eq!(d.level, Level::Error);
        assert!(d.message.contains("write-capable scope"));
    }

    #[test]
    fn job_level_write_with_top_level_read_is_clean() {
        // Spec scenario: only top-level permissions are checked; job-level write is the
        // correct location and the rule does not flag it.
        let p = parse(
            "on: push
permissions:
  contents: read
jobs:
  publish:
    permissions:
      packages: write
    steps: []
",
        );
        assert!(ExcessivePermissionsRule::check_workflow(&p).is_none());
    }

    #[test]
    fn no_permissions_block_is_clean_here_handled_by_missing_permissions() {
        let p = parse("on: push\njobs: {}\n");
        assert!(ExcessivePermissionsRule::check_workflow(&p).is_none());
    }
}

//! Workflow-security lint rules. Each rule consumes the structural `Parsed` view of a
//! workflow (via `Context::workflows_full`) and emits diagnostics carrying workflow
//! plus optional job/step location.

#![expect(clippy::pub_use, reason = "reexport rule structs to lint::command")]

/// Workflow-security: flags `pull_request_target` and `workflow_run` triggers.
mod dangerous_trigger;
/// Workflow-security: warns when top-level `permissions:` is broader than `contents: read`.
mod excessive_permissions;
/// Workflow-security: warns when a push/schedule workflow has no `concurrency:` block.
mod missing_concurrency;
/// Workflow-security: detects workflows that lack a top-level `permissions:` block.
mod missing_permissions;
/// Workflow-security: errors when a privileged workflow checks out the PR HEAD ref.
mod pr_head_checkout;
/// Workflow-security: errors when a PR workflow uses a user secret without a fork-PR gate.
mod unprotected_secrets;

pub use dangerous_trigger::DangerousTriggerRule;
pub use excessive_permissions::ExcessivePermissionsRule;
pub use missing_concurrency::MissingConcurrencyRule;
pub use missing_permissions::MissingPermissionsRule;
pub use pr_head_checkout::PrHeadCheckoutRule;
pub use unprotected_secrets::UnprotectedSecretsRule;

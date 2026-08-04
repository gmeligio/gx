//! Which schema a managed file follows.
//!
//! Kind is decided once, where a file is found — by the discovery root that matched it, or
//! by the edge that reached it — and carried from there. It is never recomputed from a
//! path: a path says where a file sits, not which schema it follows, and the two stop
//! agreeing as soon as an action definition lives outside `.github/actions`.

/// Which GitHub schema a managed file follows. The two hold `uses:` references in
/// different places — `jobs.<id>.steps` versus `runs.steps` — and only the workflow
/// schema has `on:`, `permissions:`, and jobs, so the workflow-security and validity
/// rules apply to it alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// A workflow, conventionally under `.github/workflows`.
    Workflow,
    /// An action definition, conventionally under `.github/actions`.
    ActionDefinition,
}

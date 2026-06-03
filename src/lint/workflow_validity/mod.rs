//! Workflow-validity lint rules. Each rule consumes the structural `Parsed` view of a
//! workflow (via `Context::workflows_full`) and flags references that GitHub Actions
//! accepts at parse time but that fail or silently resolve to nothing at run time.

#![expect(clippy::pub_use, reason = "reexport rule structs to lint::command")]

/// Workflow-validity: flags `needs:` entries that name a job absent from the workflow.
mod dangling_reference;

pub use dangling_reference::DanglingReferenceRule;

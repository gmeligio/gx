#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// Core lint command runner (phase orchestration + the public Lint command).
mod command;
pub mod report;
/// Rule identity (`RuleName`), `Diagnostic`/`Context`/`Rule` types, and ignore matchers.
mod rule;
/// Detects workflows where the pinned SHA does not match the lock file.
mod sha_mismatch;
/// Detects stale version comments that no longer match the locked version.
mod stale_comment;
/// Detects actions used without a pinned SHA.
mod unpinned;
/// Detects actions present in workflows but missing from the manifest.
mod unsynced_manifest;
/// Workflow-security rule family (permissions, triggers, secrets, concurrency).
mod workflow_security;

pub use command::{Error, Lint, collect_diagnostics};
pub use rule::{Context, Diagnostic, Rule, RuleName, format_and_report};

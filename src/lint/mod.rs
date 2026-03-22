#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// Core lint command logic, types, and rule orchestration.
mod command;
pub mod report;
/// Detects workflows where the pinned SHA does not match the lock file.
mod sha_mismatch;
/// Detects stale version comments that no longer match the locked version.
mod stale_comment;
/// Detects actions used without a pinned SHA.
mod unpinned;
/// Detects actions present in workflows but missing from the manifest.
mod unsynced_manifest;

pub use command::{
    Context, Diagnostic, Error, Lint, Rule, RuleName, collect_diagnostics, format_and_report,
};

#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

//! Diagnostics vocabulary shared by the commands that report findings.
//!
//! Lives at the crate root rather than inside a command module so that lower-level
//! modules (`config`, `infra`) can name a rule identity without depending on any
//! command.

/// The `rule_ids!` macro: one list generates a rule-identity enum and all its
/// string conversions.
mod identity;
/// The diagnostic record and the ignore matchers.
mod record;
/// Severity counts, exit code, and the pluralized summary line.
mod report;
/// The lint rule identity, built from `rule_ids!`.
mod rule_name;

pub use record::{Diagnostic, matches_ignore, matches_ignore_action, matches_ignore_workflow};
pub use report::Report;
pub use rule_name::RuleName;

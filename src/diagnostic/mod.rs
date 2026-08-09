#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

//! Diagnostics shared by the commands that report findings.
//!
//! At the crate root, not under a command, so `config` and `infra` can name a rule
//! without depending on one.

/// The `rule_ids!` macro. Declared first — `macro_rules!` is visible only to modules
/// declared after it.
mod identity;
/// The diagnostic record and the ignore matchers.
mod record;
/// Severity counts, exit code, and the summary line.
mod report;
/// The lint rule identity.
mod rule_name;

pub use record::Diagnostic;
pub(crate) use record::{matches_ignore, matches_ignore_action, matches_ignore_workflow};
pub use report::Report;
pub use rule_name::RuleName;

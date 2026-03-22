#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// Current two-tier format: read + write.
mod format;
/// Legacy flat format reader.
mod migration;
/// Lock file store, error types, and TOML parsing.
mod store;

use store::parse_toml;
pub use store::{Error, LOCK_FILE_NAME, Store};

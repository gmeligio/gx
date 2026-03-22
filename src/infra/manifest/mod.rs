#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// TOML serialization, deserialization, and document building for manifests.
mod convert;
/// Manifest file parsing, creation, and store.
mod parse;
pub mod patch;

pub use parse::{Error, MANIFEST_FILE_NAME, Store, create, parse, parse_lint_config};

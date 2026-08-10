#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// Per-run memoizing decorator over any [`crate::domain::resolution::VersionRegistry`].
mod caching;

pub use caching::Caching;

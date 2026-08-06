//! Forge-agnostic decorators over [`crate::domain::resolution::VersionRegistry`].
//!
//! These wrap any registry rather than any particular forge: they key off the
//! forge-neutral [`crate::domain::resolution::Error`] classification, so a second
//! backend gets the same behavior for free.
//!
//! Composed cache-outside-retry — `Caching::new(Retrying::new(registry))` — so a
//! cache hit short-circuits before any retry runs, and a wait is only ever spent
//! on a request that genuinely has to reach the forge.

#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// Per-run memoizing decorator over any [`crate::domain::resolution::VersionRegistry`].
mod caching;
/// Bounded retry with backoff for rate-limited resolution requests.
mod retrying;

pub use caching::Caching;
pub use retrying::{Retrying, Waiter};

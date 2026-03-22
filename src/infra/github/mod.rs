#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// GitHub API client, error types, and `VersionRegistry` implementation.
mod registry;
/// Ref resolution and tag lookup against the GitHub API.
mod resolve;
/// GitHub API response deserialization types.
mod responses;

pub use registry::{Error, Registry};

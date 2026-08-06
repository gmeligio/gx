#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// Commit, release, and tag date lookups against the GitHub API.
mod dates;
/// GitHub API client, error types, and `VersionRegistry` implementation.
mod registry;
/// Ref resolution: the tag, release, branch, and commit fallback chain.
mod resolve;
/// GitHub API response deserialization types.
mod responses;
/// Tag enumeration: tags for a SHA, version tags, and pagination.
mod tags;

pub use registry::{Error, Registry};

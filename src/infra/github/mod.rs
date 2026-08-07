#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// Security advisory lookups over GitHub's GraphQL API.
mod advisory;
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

pub use advisory::{Advisory, AdvisoryQuery, GraphQlAdvisories, Severity as AdvisorySeverity};
pub use registry::{Error, MAX_RETRY_WAIT_SECS, Registry};

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]

//! End-to-end GitHub registry tests using the real GitHub API.
//!
//! These tests require a `GITHUB_TOKEN` environment variable and network access.
//! Run via `mise run e2e`.

use gx::domain::action::identity::{ActionId, CommitSha};
use gx::domain::action::uses_ref::RefType;
use gx::domain::resolution::VersionRegistry as _;
use gx::infra::github::Registry as GithubRegistry;

fn github_registry() -> GithubRegistry {
    let token = std::env::var("GITHUB_TOKEN").ok();
    GithubRegistry::new(token).expect("Failed to create GithubRegistry")
}

#[test]
fn resolve_ref_returns_release_for_tag_with_release() {
    // This test requires a valid GITHUB_TOKEN to call the GitHub API
    // It verifies that a tag with an associated release returns RefType::Release
    let client = github_registry();
    // Using actions/checkout@v4.2.2 as test case (has a GitHub Release)
    let (sha, ref_type) = client.resolve_ref("actions/checkout", "v4.2.2").unwrap();
    assert!(!sha.is_empty());
    assert_eq!(ref_type, Some(RefType::Release));
}

#[test]
fn get_tags_for_sha_includes_annotated_tags() {
    let client = github_registry();
    // actions/checkout v6 is an annotated tag
    // First resolve v6 to get the commit SHA
    let (sha, _) = client.resolve_ref("actions/checkout", "v6").unwrap();
    let tags = client.get_tags_for_sha("actions/checkout", &sha).unwrap();
    // Should include both v6 and more specific versions like v6.x.y
    assert!(
        tags.iter().any(|t| t == "v6"),
        "expected v6 in tags, got: {tags:?}"
    );
}

/// `resolve_ref` for an annotated tag must return the underlying commit SHA,
/// not the tag object SHA. The tag object SHA is a git internal reference that
/// cannot be used in `uses: owner/repo@sha` workflow pins.
#[test]
fn resolve_ref_annotated_tag_returns_commit_sha_not_tag_object() {
    let client = github_registry();
    // release-plz/action@v0.5 uses an annotated tag
    let (sha, ref_type) = client.resolve_ref("release-plz/action", "v0.5").unwrap();

    // The ref type should indicate it's a tag (or release)
    assert!(
        ref_type == Some(RefType::Tag) || ref_type == Some(RefType::Release),
        "expected Tag or Release, got: {ref_type:?}"
    );

    // The returned SHA must be a valid commit, not a tag object.
    // Verify by calling describe_sha which fetches the commit date — this fails for tag objects.
    let id = ActionId::from("release-plz/action");
    let commit_sha = CommitSha::from(sha.as_str());
    let description = client.describe_sha(&id, &commit_sha);
    assert!(
        description.is_ok(),
        "SHA {sha} should be a valid commit, but describe_sha failed: {:?} \
         (likely a tag object SHA was returned instead of the commit SHA)",
        description.err()
    );
}

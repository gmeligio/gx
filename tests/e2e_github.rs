#![allow(unused_crate_dependencies)]

//! End-to-end GitHub registry tests using the real GitHub API.
//!
//! These tests require a `GITHUB_TOKEN` environment variable and network access.
//! Run via `mise run e2e`.

use gx::domain::RefType;
use gx::infra::GithubRegistry;

fn github_registry() -> GithubRegistry {
    let token = std::env::var("GITHUB_TOKEN").ok();
    GithubRegistry::new(token).expect("Failed to create GithubRegistry")
}

#[test]
fn test_resolve_ref_returns_release_for_tag_with_release() {
    // This test requires a valid GITHUB_TOKEN to call the GitHub API
    // It verifies that a tag with an associated release returns RefType::Release
    let client = github_registry();
    // Using actions/checkout@v4.2.2 as test case (has a GitHub Release)
    let (sha, ref_type) = client.resolve_ref("actions/checkout", "v4.2.2").unwrap();
    assert!(!sha.is_empty());
    assert_eq!(ref_type, Some(RefType::Release));
}

#[test]
fn test_get_tags_for_sha_includes_annotated_tags() {
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

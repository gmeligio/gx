use super::filter_refs_by_sha;
use crate::domain::{RefType, Version, VersionRegistry};
use crate::infra::github::GithubRegistry;
use crate::infra::github::responses::{GitObject, GitRefEntry};

fn make_ref_entry(ref_name: &str, sha: &str) -> GitRefEntry {
    make_ref_entry_typed(ref_name, sha, "commit")
}

fn make_ref_entry_typed(ref_name: &str, sha: &str, object_type: &str) -> GitRefEntry {
    GitRefEntry {
        ref_name: ref_name.to_string(),
        object: GitObject {
            sha: sha.to_string(),
            object_type: object_type.to_string(),
        },
    }
}

#[test]
fn test_full_sha_passthrough() {
    let client = GithubRegistry::new(None).unwrap();
    let sha = "a1b2c3d4e5f6789012345678901234567890abcd";
    let (result_sha, result_type) = client.resolve_ref("actions/checkout", sha).unwrap();
    assert_eq!(result_sha, sha);
    assert_eq!(result_type, Some(RefType::Commit));
}

#[test]
fn test_subpath_action_extracts_base_repo() {
    let client = GithubRegistry::new(None).unwrap();
    let sha = "a1b2c3d4e5f6789012345678901234567890abcd";
    // Should work with subpath actions
    let (result_sha, result_type) = client
        .resolve_ref("github/codeql-action/upload-sarif", sha)
        .unwrap();
    assert_eq!(result_sha, sha);
    assert_eq!(result_type, Some(RefType::Commit));
}

#[test]
fn test_version_resolver_trait() {
    let client = GithubRegistry::new(None).unwrap();
    let id = crate::domain::ActionId::from("actions/checkout");
    let sha_version = Version::from("a1b2c3d4e5f6789012345678901234567890abcd");

    // Full SHA should pass through
    let result = client.lookup_sha(&id, &sha_version).unwrap();
    assert_eq!(result.sha.as_str(), sha_version.as_str());
    assert_eq!(result.ref_type, Some(RefType::Commit));
}

// --- filter_refs_by_sha tests ---

#[test]
fn test_filter_refs_lightweight_tags_match_commit_sha() {
    let commit_sha = "abc123def456789012345678901234567890abcd";
    let refs = vec![
        make_ref_entry("refs/tags/v4", commit_sha),
        make_ref_entry("refs/tags/v4.2.1", commit_sha),
        make_ref_entry("refs/tags/v3", "other_sha_000000000000000000000000000"),
    ];

    let tags = filter_refs_by_sha(&refs, commit_sha);
    assert_eq!(tags, vec!["v4", "v4.2.1"]);
}

#[test]
fn test_filter_refs_no_matches() {
    let refs = vec![
        make_ref_entry("refs/tags/v4", "aaa0000000000000000000000000000000000000"),
        make_ref_entry("refs/tags/v3", "bbb0000000000000000000000000000000000000"),
    ];

    let tags = filter_refs_by_sha(&refs, "ccc0000000000000000000000000000000000000");
    assert!(tags.is_empty());
}

/// `filter_refs_by_sha` only matches lightweight tags. Annotated tags
/// `(object_type == "tag")` are handled separately by `get_tags_for_sha`
/// via dereferencing.
#[test]
fn test_filter_refs_skips_annotated_tags() {
    let commit_sha = "abc123def456789012345678901234567890abcd";
    let tag_object_sha = "fedcba9876543210fedcba9876543210fedcba98";

    let refs = vec![
        make_ref_entry_typed("refs/tags/v6", tag_object_sha, "tag"), // annotated
        make_ref_entry_typed("refs/tags/v6.2.0", tag_object_sha, "tag"), // annotated
        make_ref_entry("refs/tags/v5", commit_sha),                  // lightweight
    ];

    // filter_refs_by_sha only picks up lightweight matches
    let tags = filter_refs_by_sha(&refs, commit_sha);
    assert_eq!(tags, vec!["v5"]);
}

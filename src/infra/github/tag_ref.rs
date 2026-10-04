//! Resolving one specific tag to the commit it points at.
//!
//! Separate from [`super::resolve`], which owns the tag/branch/commit fallback chain: audit
//! already knows the entry named a tag, so guessing among ref kinds would be the wrong
//! question. The seam exists because the check it feeds decides whether a user's dependency
//! was tampered with, and that judgment must be exercised offline and deterministically.

use super::Error;
use super::Registry;
use super::resolve::GITHUB_API_BASE;
use crate::domain::action::identity::{ActionId, CommitSha};

/// Resolves a tag to the commit it currently points at.
///
/// Implemented by [`GitTags`] (real) and [`FakeTags`] (tests).
pub trait TagResolver {
    /// The commit `tag` points at in `action`'s repository, following annotated tags
    /// through to their target.
    ///
    /// An `Err` means the tag's target could not be established — never a guess, because a
    /// caller that mistook "could not look" for "unchanged" would report a false clean.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if the request fails, is rejected, or the tag does not exist.
    fn tag_commit(&self, action: &ActionId, tag: &str) -> Result<CommitSha, Error>;
}

/// Resolves tags against the live GitHub API.
pub struct GitTags {
    /// The client carrying the token and HTTP configuration.
    registry: Registry,
}

impl GitTags {
    /// Wrap a registry as a tag resolver.
    #[must_use]
    pub const fn new(registry: Registry) -> Self {
        Self { registry }
    }
}

/// The ref endpoint for `tag` in `action`'s repository.
///
/// Tags belong to the repository, not to a directory inside it, so a subpath action resolves
/// against its root — otherwise the request names a repository that does not exist and a
/// healthy dependency is reported as unverifiable.
fn tag_ref_url(action: &ActionId, tag: &str) -> String {
    format!(
        "{GITHUB_API_BASE}/repos/{}/git/ref/tags/{tag}",
        action.base_repo()
    )
}

impl TagResolver for GitTags {
    fn tag_commit(&self, action: &ActionId, tag: &str) -> Result<CommitSha, Error> {
        // fetch_ref_commit dereferences annotated tags. Reimplementing that here would put a
        // second copy of the logic that prevents this check's worst failure mode.
        self.registry
            .fetch_ref_commit(&tag_ref_url(action, tag))
            .map(CommitSha::from)
    }
}

/// Returns pre-seeded tag targets without issuing any request.
///
/// Ungated, unlike the fakes in [`super::advisory`] and [`crate::infra::shellcheck`]: the
/// audit integration tests compile against the non-test library, so a `#[cfg(test)]` fake
/// would be invisible to them. Follows `crate::domain::resolution::testutil`.
pub struct FakeTags {
    /// Commit each `(action, tag)` resolves to. A miss is a failed lookup.
    targets: std::collections::HashMap<(String, String), String>,
    /// Every lookup asked for, in call order, so a test can prove a skipped entry was
    /// never looked up — an assertion on findings alone would pass for the wrong reason.
    seen: std::cell::RefCell<Vec<(String, String)>>,
}

impl FakeTags {
    /// A resolver whose every lookup fails.
    #[must_use]
    pub fn failing() -> Self {
        Self {
            targets: std::collections::HashMap::new(),
            seen: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// Seed the commit `tag` resolves to for `action`.
    #[must_use]
    pub fn with(mut self, action: &str, tag: &str, sha: &str) -> Self {
        self.targets
            .insert((action.to_owned(), tag.to_owned()), sha.to_owned());
        self
    }

    /// Every `(action, tag)` lookup this resolver was asked for.
    #[must_use]
    pub fn lookups(&self) -> Vec<(String, String)> {
        self.seen.borrow().clone()
    }
}

impl Default for FakeTags {
    fn default() -> Self {
        Self::failing()
    }
}

impl TagResolver for FakeTags {
    fn tag_commit(&self, action: &ActionId, tag: &str) -> Result<CommitSha, Error> {
        // Keyed on the repository root, mirroring the real adapter: a fake that answered
        // for the full subpath would let a broken adapter pass.
        let key = (action.base_repo().to_string(), tag.to_owned());
        self.seen.borrow_mut().push(key.clone());
        self.targets
            .get(&key)
            .map(|sha| CommitSha::from(sha.as_str()))
            .ok_or_else(|| Error::NotFound {
                url: format!("{GITHUB_API_BASE}/repos/{}/git/ref/tags/{tag}", key.0),
            })
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{FakeTags, TagResolver as _, tag_ref_url};
    use crate::domain::action::identity::ActionId;

    #[test]
    fn tag_url_targets_the_git_ref_endpoint() {
        assert_eq!(
            tag_ref_url(&ActionId::from("actions/checkout"), "v4.2.1"),
            "https://api.github.com/repos/actions/checkout/git/ref/tags/v4.2.1"
        );
    }

    #[test]
    fn subpath_action_resolves_against_its_repository_root() {
        // The full path names no repository, so the request would 404 and the check would
        // report a healthy action as unverifiable.
        assert_eq!(
            tag_ref_url(&ActionId::from("github/codeql-action/upload-sarif"), "v3"),
            "https://api.github.com/repos/github/codeql-action/git/ref/tags/v3"
        );
    }

    #[test]
    fn the_fake_answers_for_a_seeded_tag_and_fails_otherwise() {
        let fake = FakeTags::failing().with("actions/checkout", "v4", "abc");
        let id = ActionId::from("actions/checkout");

        assert_eq!(fake.tag_commit(&id, "v4").unwrap().as_str(), "abc");
        fake.tag_commit(&id, "v5").unwrap_err();
        assert_eq!(
            fake.lookups(),
            vec![
                ("actions/checkout".to_owned(), "v4".to_owned()),
                ("actions/checkout".to_owned(), "v5".to_owned())
            ]
        );
    }
}

use super::Error as GithubError;
use super::Registry;
use super::responses::{CommitResponse, GitRef, GitTagResponse};
use crate::domain::action::identity::{ActionId, CommitSha};
use crate::domain::action::uses_ref::RefType;

/// Base URL for the GitHub REST API.
pub(super) const GITHUB_API_BASE: &str = "https://api.github.com";

/// The ref endpoint for `tag` in `base_repo`, an `owner/repo` slug.
fn tag_ref_url(base_repo: &str, tag: &str) -> String {
    format!("{GITHUB_API_BASE}/repos/{base_repo}/git/ref/tags/{tag}")
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Registry's impl is split across files to stay within the per-file size budget"
)]
impl Registry {
    /// Resolve a ref (tag, branch, or commit) to a full commit SHA and detect the ref type.
    ///
    /// Returns a tuple of (`sha`, `ref_type`) by tracking which API path succeeded.
    ///
    /// # Examples
    ///
    /// - `resolve_ref("actions/checkout", "v4") -> ("abc123...", RefType::Tag)`
    /// - `resolve_ref("actions/checkout", "main") -> ("def456...", RefType::Branch)`
    /// - `resolve_ref("actions/checkout", "abc123") -> ("abc123...", RefType::Commit)`
    /// - `resolve_ref("github/codeql-action/upload-sarif", "v4") -> ("abc123...", RefType::Tag)`
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or returns a non-success status.
    pub fn resolve_ref(
        &self,
        owner_repo: &str,
        ref_name: &str,
    ) -> Result<(String, Option<RefType>), GithubError> {
        // If it already looks like a full SHA (40 hex chars), return it as a Commit
        if CommitSha::is_valid(ref_name) {
            return Ok((ref_name.to_owned(), Some(RefType::Commit)));
        }

        // Handle subpath actions (e.g., "github/codeql-action/upload-sarif")
        // Extract just the owner/repo part (first two path segments)
        let base_repo = owner_repo.split('/').take(2).collect::<Vec<_>>().join("/");

        // Try to resolve as a tag first
        if let Ok(sha) = self.fetch_ref_commit(&tag_ref_url(&base_repo, ref_name)) {
            // Check if this tag has a GitHub Release
            if self
                .fetch_release_date(&base_repo, ref_name)
                .ok()
                .flatten()
                .is_some()
            {
                return Ok((sha, Some(RefType::Release)));
            }
            return Ok((sha, Some(RefType::Tag)));
        }

        // Try to resolve as a branch
        let branch_url = format!("{GITHUB_API_BASE}/repos/{base_repo}/git/ref/heads/{ref_name}");
        if let Ok(sha) = self.fetch_ref_commit(&branch_url) {
            return Ok((sha, Some(RefType::Branch)));
        }

        // Try to resolve as a direct commit
        let commit_url = format!("{GITHUB_API_BASE}/repos/{base_repo}/commits/{ref_name}");
        self.fetch_commit_sha(&commit_url)
            .map(|sha| (sha, Some(RefType::Commit)))
    }

    /// Fetch the commit SHA for a git ref, dereferencing annotated tags if needed.
    pub(super) fn fetch_ref_commit(&self, url: &str) -> Result<String, GithubError> {
        let git_ref: GitRef = self.get_json(url, "ref")?;

        // For annotated tags, the object is a tag object, not a commit.
        // Dereference via the git tags API to get the underlying commit SHA.
        if git_ref.object.object_type == "tag" {
            let tag_url = format!(
                "{GITHUB_API_BASE}/repos/{}/git/tags/{}",
                // Extract owner/repo from the ref URL
                url.strip_prefix(&format!("{GITHUB_API_BASE}/repos/"))
                    .and_then(|s| {
                        let mut split = s.splitn(3, '/');
                        let owner = split.next()?;
                        let repo = split.next()?;
                        Some(format!("{owner}/{repo}"))
                    })
                    .unwrap_or_default(),
                git_ref.object.sha
            );

            let tag_data: GitTagResponse = self.get_json(&tag_url, "tag dereference")?;

            return Ok(tag_data.object.sha);
        }

        Ok(git_ref.object.sha)
    }

    /// Fetch the SHA from a commit endpoint URL.
    pub(super) fn fetch_commit_sha(&self, url: &str) -> Result<String, GithubError> {
        let commit: CommitResponse = self.get_json(url, "commit")?;
        Ok(commit.sha)
    }
}

/// Resolves a tag to the commit it currently points at.
///
/// Implemented by [`GitTags`] (real) and `FakeTags` (tests).
pub trait TagResolver {
    /// The commit `tag` points at in `action`'s repository, following annotated tags
    /// through to their target.
    ///
    /// An `Err` means the tag's target could not be established — never a guess, because a
    /// caller that mistook "could not look" for "unchanged" would report a false clean.
    ///
    /// # Errors
    ///
    /// Returns [`GithubError`] if the request fails, is rejected, or the tag does not exist.
    fn tag_commit(&self, action: &ActionId, tag: &str) -> Result<CommitSha, GithubError>;
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

impl TagResolver for GitTags {
    fn tag_commit(&self, action: &ActionId, tag: &str) -> Result<CommitSha, GithubError> {
        // Tags belong to the repository, so a subpath action resolves against its root.
        self.registry
            .fetch_ref_commit(&tag_ref_url(action.base_repo().as_str(), tag))
            .map(CommitSha::from)
    }
}

/// The [`TagResolver`] test double.
///
/// Inline because `src/infra/github/` is at its 8-file budget.
#[cfg(test)]
pub(crate) mod fake {
    use super::{ActionId, CommitSha, GithubError, TagResolver, tag_ref_url};
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// Answers tag lookups from memory without issuing any request.
    pub struct FakeTags {
        /// What an unseeded lookup returns; `None` fails it.
        fallback: Option<String>,
        /// Commit each `(repository, tag)` resolves to, overriding `fallback`.
        targets: HashMap<(String, String), String>,
        /// Every `(repository, tag)` asked for, in call order, so a test can prove a
        /// skipped entry was never looked up.
        seen: RefCell<Vec<(String, String)>>,
    }

    impl FakeTags {
        /// A resolver whose every tag points at `sha` — no tag has moved.
        #[must_use]
        pub fn resolving(sha: &str) -> Self {
            Self::from_fallback(Some(sha.to_owned()))
        }

        /// A resolver whose every lookup fails.
        #[must_use]
        pub fn failing() -> Self {
            Self::from_fallback(None)
        }

        /// Shared constructor.
        fn from_fallback(fallback: Option<String>) -> Self {
            Self {
                fallback,
                targets: HashMap::new(),
                seen: RefCell::new(Vec::new()),
            }
        }

        /// Seed the commit `tag` resolves to in `repo`.
        #[must_use]
        pub fn with(mut self, repo: &str, tag: &str, sha: &str) -> Self {
            self.targets
                .insert((repo.to_owned(), tag.to_owned()), sha.to_owned());
            self
        }

        /// Every `(repository, tag)` lookup this resolver was asked for.
        #[must_use]
        pub fn lookups(&self) -> Vec<(String, String)> {
            self.seen.borrow().clone()
        }
    }

    impl TagResolver for FakeTags {
        fn tag_commit(&self, action: &ActionId, tag: &str) -> Result<CommitSha, GithubError> {
            // Keyed on the repository root, as the real adapter is.
            let key = (action.base_repo().as_str().to_owned(), tag.to_owned());
            self.seen.borrow_mut().push(key.clone());
            self.targets
                .get(&key)
                .or(self.fallback.as_ref())
                .map(|sha| CommitSha::from(sha.as_str()))
                .ok_or_else(|| GithubError::NotFound {
                    url: tag_ref_url(&key.0, tag),
                })
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::Registry as GithubRegistry;
    use super::tag_ref_url;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::resolution::VersionRegistry as _;

    #[test]
    fn full_sha_passthrough() {
        let client = GithubRegistry::new(None).unwrap();
        let sha = "a1b2c3d4e5f6789012345678901234567890abcd";
        let (result_sha, result_type) = client.resolve_ref("actions/checkout", sha).unwrap();
        assert_eq!(result_sha, sha);
        assert_eq!(result_type, Some(RefType::Commit));
    }

    #[test]
    fn subpath_action_extracts_base_repo() {
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
    fn version_resolver_preserves_commit_when_date_is_unavailable() {
        // Port zero refuses connections; the explicit proxy prevents the optional date
        // lookup from reaching GitHub or depending on this machine's proxy settings.
        let client = GithubRegistry {
            client: reqwest::blocking::Client::builder()
                .no_proxy()
                .proxy(reqwest::Proxy::all("http://127.0.0.1:0").unwrap())
                .timeout(std::time::Duration::from_secs(1))
                .build()
                .unwrap(),
            token: None,
        };
        let id = crate::domain::action::identity::ActionId::from("actions/checkout");
        let sha_version = crate::domain::action::identity::Version::from(
            "a1b2c3d4e5f6789012345678901234567890abcd",
        );

        let result = client.lookup_sha(&id, &sha_version).unwrap();
        assert_eq!(result.sha.as_str(), sha_version.as_str());
        assert_eq!(result.ref_type, Some(RefType::Commit));
        assert!(result.date.as_str().is_empty());
    }

    #[test]
    fn tag_url_targets_the_repository_root() {
        for (action, tag, url) in [
            (
                "actions/checkout",
                "v4.2.1",
                "https://api.github.com/repos/actions/checkout/git/ref/tags/v4.2.1",
            ),
            (
                "github/codeql-action/upload-sarif",
                "v3",
                "https://api.github.com/repos/github/codeql-action/git/ref/tags/v3",
            ),
        ] {
            let id = crate::domain::action::identity::ActionId::from(action);
            assert_eq!(tag_ref_url(id.base_repo().as_str(), tag), url);
        }
    }
}

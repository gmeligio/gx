use super::Error as GithubError;
use super::Registry;
use super::responses::{CommitResponse, GitRef, GitTagResponse};
use crate::domain::action::identity::CommitSha;
use crate::domain::action::uses_ref::RefType;

/// Base URL for the GitHub REST API.
pub(super) const GITHUB_API_BASE: &str = "https://api.github.com";

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
        let tag_url = format!("{GITHUB_API_BASE}/repos/{base_repo}/git/ref/tags/{ref_name}");
        if let Ok(sha) = self.fetch_ref_commit(&tag_url) {
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

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::Registry as GithubRegistry;
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
    fn version_resolver_trait() {
        let client = GithubRegistry::new(None).unwrap();
        let id = crate::domain::action::identity::ActionId::from("actions/checkout");
        let sha_version = crate::domain::action::identity::Version::from(
            "a1b2c3d4e5f6789012345678901234567890abcd",
        );

        // Full SHA should pass through
        let result = client.lookup_sha(&id, &sha_version).unwrap();
        assert_eq!(result.sha.as_str(), sha_version.as_str());
        assert_eq!(result.ref_type, Some(RefType::Commit));
    }
}

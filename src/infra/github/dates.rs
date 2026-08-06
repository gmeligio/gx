use super::Error as GithubError;
use super::Registry;
use super::resolve::GITHUB_API_BASE;
use super::responses::{CommitDetailResponse, ReleaseResponse, TagObjectResponse};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "date lookup is in a separate file for clarity"
)]
impl Registry {
    /// Fetch the commit date from a commit SHA.
    ///
    /// # Errors
    ///
    /// Returns an error if no token is set, the request fails, or the response cannot be parsed.
    pub(super) fn fetch_commit_date(
        &self,
        base_repo: &str,
        sha: &str,
    ) -> Result<Option<String>, GithubError> {
        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/commits/{sha}");
        let commit: CommitDetailResponse = self.get_json(&url, "commit details")?;
        Ok(commit.commit.committer.and_then(|c| c.date))
    }

    /// Fetch the release date from a release tag.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response cannot be parsed.
    pub(super) fn fetch_release_date(
        &self,
        base_repo: &str,
        tag: &str,
    ) -> Result<Option<String>, GithubError> {
        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/releases/tags/{tag}");
        let release: ReleaseResponse = self.get_json(&url, "release")?;
        Ok(release.published_at)
    }

    /// Fetch the tag date from an annotated tag object.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response cannot be parsed.
    pub(super) fn fetch_tag_date(
        &self,
        base_repo: &str,
        sha: &str,
    ) -> Result<Option<String>, GithubError> {
        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/git/tags/{sha}");
        let tag: TagObjectResponse = self.get_json(&url, "tag")?;
        Ok(tag.tagger.and_then(|t| t.date))
    }
}

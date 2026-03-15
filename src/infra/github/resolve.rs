use super::Error as GithubError;
use super::Registry;
use super::responses::{
    CommitDetailResponse, CommitResponse, GitRef, GitRefEntry, GitTagResponse, ReleaseResponse,
    TagObjectResponse,
};
use crate::domain::action::identity::CommitSha;
use crate::domain::action::uses_ref::RefType;

/// Base URL for the GitHub REST API.
const GITHUB_API_BASE: &str = "https://api.github.com";

#[expect(
    clippy::multiple_inherent_impl,
    reason = "resolution logic is in a separate file for clarity"
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
        let response =
            self.authenticated_get(url)
                .send()
                .map_err(|source| GithubError::Request {
                    operation: "ref",
                    url: url.to_owned(),
                    source,
                })?;

        if !response.status().is_success() {
            return Err(Self::check_status(&response, url));
        }

        let git_ref: GitRef = response
            .json()
            .map_err(|source| GithubError::ParseResponse {
                url: url.to_owned(),
                source,
            })?;

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

            let tag_response =
                self.authenticated_get(&tag_url)
                    .send()
                    .map_err(|source| GithubError::Request {
                        operation: "tag dereference",
                        url: tag_url.clone(),
                        source,
                    })?;

            if !tag_response.status().is_success() {
                return Err(Self::check_status(&tag_response, &tag_url));
            }

            let tag_data: GitTagResponse =
                tag_response
                    .json()
                    .map_err(|source| GithubError::ParseResponse {
                        url: tag_url,
                        source,
                    })?;

            return Ok(tag_data.object.sha);
        }

        Ok(git_ref.object.sha)
    }

    /// Fetch the SHA from a commit endpoint URL.
    pub(super) fn fetch_commit_sha(&self, url: &str) -> Result<String, GithubError> {
        let response =
            self.authenticated_get(url)
                .send()
                .map_err(|source| GithubError::Request {
                    operation: "commit",
                    url: url.to_owned(),
                    source,
                })?;

        if !response.status().is_success() {
            return Err(Self::check_status(&response, url));
        }

        let commit: CommitResponse =
            response
                .json()
                .map_err(|source| GithubError::ParseResponse {
                    url: url.to_owned(),
                    source,
                })?;

        Ok(commit.sha)
    }

    /// Get all tags that point to a specific commit SHA.
    ///
    /// Returns tag names without the "refs/tags/" prefix (e.g., `["v5", "v5.0.0"]`)
    /// Handles both lightweight tags (where `object.sha` is the commit SHA directly)
    /// and annotated tags (where `object.sha` is the tag object SHA, requiring
    /// dereferencing via `git/tags/{tag_sha}` to find the underlying commit SHA).
    ///
    /// # Errors
    ///
    /// Returns an error if no token is set, the request fails, or the response cannot be parsed.
    pub fn get_tags_for_sha(
        &self,
        owner_repo: &str,
        sha: &str,
    ) -> Result<Vec<String>, GithubError> {
        // Handle subpath actions (e.g., "github/codeql-action/upload-sarif")
        let base_repo = owner_repo.split('/').take(2).collect::<Vec<_>>().join("/");

        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/git/refs/tags");

        let response =
            self.authenticated_get(&url)
                .send()
                .map_err(|source| GithubError::Request {
                    operation: "tags",
                    url: url.clone(),
                    source,
                })?;

        if !response.status().is_success() {
            return Err(Self::check_status(&response, &url));
        }

        let refs: Vec<GitRefEntry> =
            response
                .json()
                .map_err(|source| GithubError::ParseResponse {
                    url: url.clone(),
                    source,
                })?;

        // Collect lightweight tag matches directly
        let mut tags = filter_refs_by_sha(&refs, sha);

        // Dereference annotated tags to check if they point to the target commit
        for entry in &refs {
            if entry.object.object_type == "tag"
                && entry.object.sha != sha
                && let Some(tag_name) = self.dereference_tag(&base_repo, entry, sha)
            {
                tags.push(tag_name);
            }
        }

        Ok(tags)
    }

    /// Dereference an annotated tag to check if it points to the given commit SHA.
    /// Returns `Some(tag_name)` if the tag's underlying commit matches, `None` otherwise.
    pub(super) fn dereference_tag(
        &self,
        base_repo: &str,
        entry: &GitRefEntry,
        commit_sha: &str,
    ) -> Option<String> {
        let tag_url = format!(
            "{GITHUB_API_BASE}/repos/{base_repo}/git/tags/{}",
            entry.object.sha
        );
        let tag_response = self.authenticated_get(&tag_url).send().ok()?;

        if !tag_response.status().is_success() {
            return None;
        }

        let tag_data: GitTagResponse = tag_response.json().ok()?;
        (tag_data.object.sha == commit_sha).then(|| {
            entry
                .ref_name
                .strip_prefix("refs/tags/")
                .unwrap_or(&entry.ref_name)
                .to_owned()
        })
    }

    /// Fetch all version-like tags using the matching-refs endpoint.
    /// Uses `GET /repos/{owner}/{repo}/git/matching-refs/tags/v` to narrow
    /// results to tags starting with "v" (semver convention).
    /// Handles pagination via Link header.
    ///
    /// # Errors
    ///
    /// Returns an error if no token is set, the request fails, or the response cannot be parsed.
    pub fn get_version_tags(&self, owner_repo: &str) -> Result<Vec<String>, GithubError> {
        let base_repo = owner_repo.split('/').take(2).collect::<Vec<_>>().join("/");

        let mut all_refs: Vec<GitRefEntry> = Vec::new();
        let mut url =
            format!("{GITHUB_API_BASE}/repos/{base_repo}/git/matching-refs/tags/v?per_page=100");

        loop {
            let response =
                self.authenticated_get(&url)
                    .send()
                    .map_err(|source| GithubError::Request {
                        operation: "version tags",
                        url: url.clone(),
                        source,
                    })?;

            if !response.status().is_success() {
                return Err(Self::check_status(&response, &url));
            }

            let next_url = parse_next_link(response.headers());

            let page: Vec<GitRefEntry> =
                response
                    .json()
                    .map_err(|source| GithubError::ParseResponse {
                        url: url.clone(),
                        source,
                    })?;

            all_refs.extend(page);

            match next_url {
                Some(next) => url = next,
                None => break,
            }
        }

        let tags: Vec<String> = all_refs
            .into_iter()
            .map(|r| {
                r.ref_name
                    .strip_prefix("refs/tags/")
                    .unwrap_or(&r.ref_name)
                    .to_owned()
            })
            .collect();

        Ok(tags)
    }

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

        let response =
            self.authenticated_get(&url)
                .send()
                .map_err(|source| GithubError::Request {
                    operation: "commit details",
                    url: url.clone(),
                    source,
                })?;

        if !response.status().is_success() {
            return Err(Self::check_status(&response, &url));
        }

        let commit: CommitDetailResponse = response
            .json()
            .map_err(|source| GithubError::ParseResponse { url, source })?;

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

        let response =
            self.authenticated_get(&url)
                .send()
                .map_err(|source| GithubError::Request {
                    operation: "release",
                    url: url.clone(),
                    source,
                })?;

        if !response.status().is_success() {
            return Err(Self::check_status(&response, &url));
        }

        let release: ReleaseResponse = response
            .json()
            .map_err(|source| GithubError::ParseResponse { url, source })?;

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

        let response =
            self.authenticated_get(&url)
                .send()
                .map_err(|source| GithubError::Request {
                    operation: "tag",
                    url: url.clone(),
                    source,
                })?;

        if !response.status().is_success() {
            return Err(Self::check_status(&response, &url));
        }

        let tag: TagObjectResponse = response
            .json()
            .map_err(|source| GithubError::ParseResponse { url, source })?;

        Ok(tag.tagger.and_then(|t| t.date))
    }
}

/// Parse the `Link` header to find the `rel="next"` URL for pagination.
pub(super) fn parse_next_link(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let link_header = headers.get("link")?.to_str().ok()?;
    for part in link_header.split(',') {
        let trimmed_part = part.trim();
        if trimmed_part.ends_with("rel=\"next\"") {
            // Extract URL between < and >
            let after_open = trimmed_part.split_once('<')?.1;
            let url_str = after_open.split_once('>')?.0;
            return Some(url_str.to_owned());
        }
    }
    None
}

/// Filter git ref entries to find lightweight tags pointing to a specific commit SHA.
/// Returns tag names without the "refs/tags/" prefix.
///
/// Only matches lightweight tags where `object.sha` is the commit SHA directly.
/// Annotated tags (`object_type` == "tag") must be dereferenced separately.
pub(super) fn filter_refs_by_sha(refs: &[GitRefEntry], sha: &str) -> Vec<String> {
    refs.iter()
        .filter(|r| r.object.sha == sha)
        .map(|r| {
            r.ref_name
                .strip_prefix("refs/tags/")
                .unwrap_or(&r.ref_name)
                .to_owned()
        })
        .collect()
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
#[path = "tests.rs"]
mod tests;

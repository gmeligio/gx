use super::Error as GithubError;
use super::Registry;
use super::registry::GITHUB_API_BASE;
use super::responses::{GitRefEntry, GitTagResponse};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Registry's impl is split across files to stay within the per-file size budget"
)]
impl Registry {
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

        let refs: Vec<GitRefEntry> = self.get_json(&url, "tags")?;

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
    ///
    /// Not `get_json`: a failed dereference must stay non-fatal.
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
    /// Not `get_json`: the `Link` header must be read before the body.
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
}

/// Parse the `Link` header to find the `rel="next"` URL for pagination.
fn parse_next_link(headers: &reqwest::header::HeaderMap) -> Option<String> {
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
fn filter_refs_by_sha(refs: &[GitRefEntry], sha: &str) -> Vec<String> {
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
mod tests {
    use super::filter_refs_by_sha;
    use crate::infra::github::responses::{GitObject, GitRefEntry};

    fn make_ref_entry(ref_name: &str, sha: &str) -> GitRefEntry {
        make_ref_entry_typed(ref_name, sha, "commit")
    }

    fn make_ref_entry_typed(ref_name: &str, sha: &str, object_type: &str) -> GitRefEntry {
        GitRefEntry {
            ref_name: ref_name.to_owned(),
            object: GitObject {
                sha: sha.to_owned(),
                object_type: object_type.to_owned(),
            },
        }
    }

    #[test]
    fn filter_refs_lightweight_tags_match_commit_sha() {
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
    fn filter_refs_no_matches() {
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
    fn filter_refs_skips_annotated_tags() {
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
}

use serde::Deserialize;

/// Git ref structure returned by the Github API.
#[derive(Debug, Deserialize)]
pub struct GitRef {
    /// The referenced git object.
    pub object: GitObject,
}

/// Git object containing a SHA and type.
#[derive(Debug, Deserialize)]
pub struct GitObject {
    /// The object's SHA hash.
    pub sha: String,
    /// The object type (e.g., `"commit"`, `"tag"`).
    #[serde(rename = "type", default)]
    pub object_type: String,
}

/// Structure for git ref entries returned by the refs API.
#[derive(Debug, Deserialize)]
pub struct GitRefEntry {
    /// The full ref name (e.g., `"refs/tags/v4"`).
    #[serde(rename = "ref")]
    pub ref_name: String,
    /// The referenced git object.
    pub object: GitObject,
}

/// Response from `GET /repos/{owner}/{repo}/commits/{ref}`.
#[derive(Deserialize)]
pub(super) struct CommitResponse {
    /// The commit SHA.
    pub sha: String,
}

/// Response from `GET /repos/{owner}/{repo}/git/tags/{sha}` for annotated tags.
/// The `object.sha` field contains the underlying commit SHA.
#[derive(Deserialize)]
pub(super) struct GitTagResponse {
    /// The tag's target object.
    pub object: GitObject,
}

/// Response for a release API call.
#[derive(Debug, Deserialize)]
pub(super) struct ReleaseResponse {
    /// When the release was published.
    #[serde(rename = "published_at")]
    pub published_at: Option<String>,
}

/// Response for a commit details API call.
#[derive(Debug, Deserialize)]
pub(super) struct CommitDetailResponse {
    /// The commit metadata.
    pub commit: CommitObject,
}

/// Commit object containing committer info.
#[derive(Debug, Deserialize)]
pub(super) struct CommitObject {
    /// Committer information, if available.
    pub committer: Option<CommitterInfo>,
}

/// Committer info from commit details.
#[derive(Debug, Deserialize)]
pub(super) struct CommitterInfo {
    /// RFC 3339 timestamp of the commit.
    pub date: Option<String>,
}

/// Response for a tag object API call.
#[derive(Debug, Deserialize)]
pub(super) struct TagObjectResponse {
    /// Tagger information, if available.
    pub tagger: Option<TaggerInfo>,
}

/// Tagger info from tag object.
#[derive(Debug, Deserialize)]
pub(super) struct TaggerInfo {
    /// RFC 3339 timestamp of the tag.
    pub date: Option<String>,
}

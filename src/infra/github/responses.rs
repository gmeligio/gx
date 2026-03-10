use serde::Deserialize;

/// Git ref structure returned by the Github API
#[derive(Debug, Deserialize)]
pub struct GitRef {
    pub object: GitObject,
}

/// Git object containing a SHA and type
#[derive(Debug, Deserialize)]
pub struct GitObject {
    pub sha: String,
    #[serde(rename = "type", default)]
    pub object_type: String,
}

/// Structure for git ref entries returned by the refs API
#[derive(Debug, Deserialize)]
pub struct GitRefEntry {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub object: GitObject,
}

#[derive(Deserialize)]
pub(super) struct CommitResponse {
    pub(super) sha: String,
}

/// Response from `GET /repos/{owner}/{repo}/git/tags/{sha}` for annotated tags.
/// The `object.sha` field contains the underlying commit SHA.
#[derive(Deserialize)]
pub(super) struct GitTagResponse {
    pub(super) object: GitObject,
}

/// Response for a release API call
#[derive(Debug, Deserialize)]
pub(super) struct ReleaseResponse {
    #[serde(rename = "published_at")]
    pub(super) published_at: Option<String>,
}

/// Response for a commit details API call
#[derive(Debug, Deserialize)]
pub(super) struct CommitDetailResponse {
    pub(super) commit: CommitObject,
}

/// Commit object containing committer info
#[derive(Debug, Deserialize)]
pub(super) struct CommitObject {
    pub(super) committer: Option<CommitterInfo>,
}

/// Committer info from commit details
#[derive(Debug, Deserialize)]
pub(super) struct CommitterInfo {
    pub(super) date: Option<String>,
}

/// Response for a tag object API call
#[derive(Debug, Deserialize)]
pub(super) struct TagObjectResponse {
    pub(super) tagger: Option<TaggerInfo>,
}

/// Tagger info from tag object
#[derive(Debug, Deserialize)]
pub(super) struct TaggerInfo {
    pub(super) date: Option<String>,
}

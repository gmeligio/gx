#![expect(
    dead_code,
    reason = "shared test helpers: not every integration test crate uses every item"
)]
use gx::domain::action::identity::{ActionId, CommitDate, CommitSha, Version};
use gx::domain::action::spec::Spec as ActionSpec;
use gx::domain::action::specifier::Specifier;
use gx::domain::action::uses_ref::RefType;
use gx::domain::resolution::{
    Error as ResolutionError, ResolvedRef, ShaDescription, VersionRegistry,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash as _, Hasher as _};

/// A versatile mock registry that resolves any version to a deterministic SHA.
///
/// Supports builder methods to configure tag lookups for both `all_tags` and
/// `tags_for_sha`/`describe_sha`. Default behavior returns empty tag lists.
#[derive(Clone, Default)]
pub struct FakeRegistry {
    /// Maps `action_id` → list of available version tags (for `all_tags`).
    tags: std::collections::HashMap<String, Vec<String>>,
    /// Maps `(action_id, sha)` → list of tags pointing to that SHA (for `tags_for_sha` / `describe_sha`).
    sha_tags: std::collections::HashMap<(String, String), Vec<String>>,
}

impl FakeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the available version tags for an action (used by `all_tags`).
    pub fn with_all_tags(mut self, id: &str, tags: Vec<&str>) -> Self {
        self.tags
            .insert(id.to_owned(), tags.into_iter().map(String::from).collect());
        self
    }

    /// Register that a specific SHA has the given tags pointing to it
    /// (used by `tags_for_sha` and `describe_sha`).
    pub fn with_sha_tags(mut self, id: &str, sha: &str, tags: Vec<&str>) -> Self {
        self.sha_tags.insert(
            (id.to_owned(), sha.to_owned()),
            tags.into_iter().map(String::from).collect(),
        );
        self
    }

    /// Generate a deterministic fake SHA (exactly 40 hex chars) from action id and version.
    pub fn fake_sha(id: &str, version: &str) -> String {
        let mut hasher = DefaultHasher::new();
        id.hash(&mut hasher);
        version.hash(&mut hasher);
        let h1 = hasher.finish();
        h1.hash(&mut hasher);
        let h2 = hasher.finish();
        h2.hash(&mut hasher);
        let h3 = hasher.finish();
        let full = format!("{h1:016x}{h2:016x}{h3:016x}");
        full[..40].to_string()
    }
}

impl VersionRegistry for FakeRegistry {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<ResolvedRef, ResolutionError> {
        Ok(ResolvedRef::new(
            CommitSha::from(Self::fake_sha(id.as_str(), version.as_str())),
            id.base_repo(),
            Some(RefType::Tag),
            CommitDate::from("2026-01-01T00:00:00Z"),
        ))
    }

    fn tags_for_sha(
        &self,
        id: &ActionId,
        sha: &CommitSha,
    ) -> Result<Vec<Version>, ResolutionError> {
        let key = (id.as_str().to_owned(), sha.as_str().to_owned());
        Ok(self
            .sha_tags
            .get(&key)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(Version::from)
            .collect())
    }

    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        Ok(self
            .tags
            .get(id.as_str())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(Version::from)
            .collect())
    }

    fn describe_sha(
        &self,
        id: &ActionId,
        sha: &CommitSha,
    ) -> Result<ShaDescription, ResolutionError> {
        let key = (id.as_str().to_owned(), sha.as_str().to_owned());
        let tags = self
            .sha_tags
            .get(&key)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(Version::from)
            .collect();
        Ok(ShaDescription {
            tags,
            repository: id.base_repo(),
            date: CommitDate::from("2026-01-01T00:00:00Z"),
        })
    }
}

/// A no-op registry that always returns `AuthRequired` (simulates missing `GITHUB_TOKEN`).
#[derive(Clone, Copy)]
pub struct AuthRequiredRegistry;

impl VersionRegistry for AuthRequiredRegistry {
    fn lookup_sha(
        &self,
        _id: &ActionId,
        _version: &Version,
    ) -> Result<ResolvedRef, ResolutionError> {
        Err(ResolutionError::AuthRequired)
    }

    fn tags_for_sha(
        &self,
        _id: &ActionId,
        _sha: &CommitSha,
    ) -> Result<Vec<Version>, ResolutionError> {
        Err(ResolutionError::AuthRequired)
    }

    fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        Err(ResolutionError::AuthRequired)
    }

    fn describe_sha(
        &self,
        _id: &ActionId,
        _sha: &CommitSha,
    ) -> Result<ShaDescription, ResolutionError> {
        Err(ResolutionError::AuthRequired)
    }
}

/// Registry where `describe_sha` returns `Ok` with an empty date string,
/// simulating a `fetch_commit_date` failure handled gracefully.
#[derive(Clone)]
pub struct EmptyDateRegistry;

impl VersionRegistry for EmptyDateRegistry {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<ResolvedRef, ResolutionError> {
        Ok(ResolvedRef::new(
            CommitSha::from(FakeRegistry::fake_sha(id.as_str(), version.as_str())),
            id.base_repo(),
            Some(RefType::Tag),
            CommitDate::from(""),
        ))
    }

    fn tags_for_sha(
        &self,
        _id: &ActionId,
        _sha: &CommitSha,
    ) -> Result<Vec<Version>, ResolutionError> {
        Ok(vec![])
    }

    fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        Ok(vec![])
    }

    fn describe_sha(
        &self,
        id: &ActionId,
        _sha: &CommitSha,
    ) -> Result<ShaDescription, ResolutionError> {
        Ok(ShaDescription {
            tags: vec![],
            repository: id.base_repo(),
            date: CommitDate::from(""),
        })
    }
}

/// Registry where `describe_sha` returns an error, simulating a fatal API failure
/// (e.g., GitHub returns 422 Unprocessable Entity).
#[derive(Clone)]
pub struct FailingDescribeRegistry;

impl VersionRegistry for FailingDescribeRegistry {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<ResolvedRef, ResolutionError> {
        Ok(ResolvedRef::new(
            CommitSha::from(FakeRegistry::fake_sha(id.as_str(), version.as_str())),
            id.base_repo(),
            Some(RefType::Tag),
            CommitDate::from("2026-01-01T00:00:00Z"),
        ))
    }

    fn tags_for_sha(
        &self,
        _id: &ActionId,
        _sha: &CommitSha,
    ) -> Result<Vec<Version>, ResolutionError> {
        Ok(vec![])
    }

    fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        Ok(vec![])
    }

    fn describe_sha(
        &self,
        id: &ActionId,
        sha: &CommitSha,
    ) -> Result<ShaDescription, ResolutionError> {
        Err(ResolutionError::ResolveFailed {
            spec: ActionSpec::new(id.clone(), Specifier::from_v1(sha.as_str())),
            reason: "Github API returned status 422 Unprocessable Entity".to_owned(),
        })
    }
}

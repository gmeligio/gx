use super::{Error as ResolutionError, ResolvedRef, ShaDescription, VersionRegistry};
use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Version};
use crate::domain::action::uses_ref::RefType;

/// Registry that always fails with `AuthRequired` on every method.
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

/// Flexible fake registry for tests. Configure via builder methods.
pub struct FakeRegistry {
    tags: std::collections::HashMap<String, (String, Vec<Version>)>,
    fixed_sha: Option<String>,
    fail_tags: bool,
}

impl FakeRegistry {
    pub fn new() -> Self {
        Self {
            tags: std::collections::HashMap::new(),
            fixed_sha: None,
            fail_tags: false,
        }
    }

    /// Configure all tag methods for `id` to return `tags`.
    pub fn with_all_tags(mut self, id: &str, tags: Vec<&str>) -> Self {
        let sha = tags.first().map_or("", |t| *t).to_owned();
        self.tags.insert(
            id.to_owned(),
            (sha, tags.into_iter().map(Version::from).collect()),
        );
        self
    }

    /// Configure tags for `id` with a specific SHA for `lookup_sha`.
    pub fn with_sha_tags(mut self, id: &str, sha: &str, tags: Vec<&str>) -> Self {
        self.tags.insert(
            id.to_owned(),
            (
                sha.to_owned(),
                tags.into_iter().map(Version::from).collect(),
            ),
        );
        self
    }

    /// Make `lookup_sha` always return this SHA for any action.
    pub fn with_fixed_sha(mut self, sha: &str) -> Self {
        self.fixed_sha = Some(sha.to_owned());
        self
    }

    /// Make `tags_for_sha` and `all_tags` return `AuthRequired`.
    pub fn fail_tags(mut self) -> Self {
        self.fail_tags = true;
        self
    }
}

impl VersionRegistry for FakeRegistry {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<ResolvedRef, ResolutionError> {
        let sha = self.fixed_sha.as_ref().map_or_else(
            || {
                self.tags
                    .get(id.as_str())
                    .map_or_else(|| version.as_str().to_owned(), |(sha, _)| sha.clone())
            },
            Clone::clone,
        );
        Ok(ResolvedRef::new(
            CommitSha::from(sha),
            id.base_repo(),
            Some(RefType::Tag),
            CommitDate::from("2026-01-01T00:00:00Z"),
        ))
    }

    fn tags_for_sha(
        &self,
        id: &ActionId,
        _sha: &CommitSha,
    ) -> Result<Vec<Version>, ResolutionError> {
        if self.fail_tags {
            return Err(ResolutionError::AuthRequired);
        }
        Ok(self
            .tags
            .get(id.as_str())
            .map(|(_, tags)| tags.clone())
            .unwrap_or_default())
    }

    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        if self.fail_tags {
            return Err(ResolutionError::AuthRequired);
        }
        Ok(self
            .tags
            .get(id.as_str())
            .map(|(_, tags)| tags.clone())
            .unwrap_or_default())
    }

    fn describe_sha(
        &self,
        id: &ActionId,
        _sha: &CommitSha,
    ) -> Result<ShaDescription, ResolutionError> {
        let tags = if self.fail_tags {
            vec![]
        } else {
            self.tags
                .get(id.as_str())
                .map(|(_, tags)| tags.clone())
                .unwrap_or_default()
        };
        Ok(ShaDescription {
            tags,
            repository: id.base_repo(),
            date: CommitDate::from("2026-01-01T00:00:00Z"),
        })
    }
}

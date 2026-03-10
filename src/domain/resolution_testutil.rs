use super::{
    ActionId, CommitSha, RefType, ResolutionError, ResolvedRef, ShaDescription, Version,
    VersionRegistry,
};

/// Registry that always fails with `AuthRequired` on every method.
pub(crate) struct AuthRequiredRegistry;

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
pub(crate) struct FakeRegistry {
    tags: std::collections::HashMap<String, (String, Vec<Version>)>,
    fixed_sha: Option<String>,
    fail_tags: bool,
}

impl FakeRegistry {
    pub(crate) fn new() -> Self {
        Self {
            tags: std::collections::HashMap::new(),
            fixed_sha: None,
            fail_tags: false,
        }
    }

    /// Configure all tag methods for `id` to return `tags`.
    pub(crate) fn with_all_tags(mut self, id: &str, tags: Vec<&str>) -> Self {
        let sha = tags.first().map_or("", |t| *t).to_string();
        self.tags.insert(
            id.to_string(),
            (sha, tags.into_iter().map(Version::from).collect()),
        );
        self
    }

    /// Configure tags for `id` with a specific SHA for `lookup_sha`.
    pub(crate) fn with_sha_tags(mut self, id: &str, sha: &str, tags: Vec<&str>) -> Self {
        self.tags.insert(
            id.to_string(),
            (
                sha.to_string(),
                tags.into_iter().map(Version::from).collect(),
            ),
        );
        self
    }

    /// Make `lookup_sha` always return this SHA for any action.
    pub(crate) fn with_fixed_sha(mut self, sha: &str) -> Self {
        self.fixed_sha = Some(sha.to_string());
        self
    }

    /// Make `tags_for_sha` and `all_tags` return `AuthRequired`.
    pub(crate) fn fail_tags(mut self) -> Self {
        self.fail_tags = true;
        self
    }
}

impl VersionRegistry for FakeRegistry {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<ResolvedRef, ResolutionError> {
        let sha = if let Some(fixed) = &self.fixed_sha {
            fixed.clone()
        } else if let Some((sha, _)) = self.tags.get(id.as_str()) {
            sha.clone()
        } else {
            version.as_str().to_string()
        };
        Ok(ResolvedRef::new(
            CommitSha::from(sha),
            id.base_repo(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
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
            date: "2026-01-01T00:00:00Z".to_string(),
        })
    }
}

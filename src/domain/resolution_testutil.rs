//! A single configurable in-memory [`VersionRegistry`] for tests.
//!
//! One fake, one contract. Every scenario a test needs is a builder call on
//! [`FakeRegistry`] rather than a bespoke type, so adding a trait method means
//! editing this file and nothing else.
//!
//! The fake mirrors the production contract deliberately. In particular
//! [`FakeRegistry::describe_sha`] is keyed on the `(action, sha)` pair, exactly as
//! `infra::github::Registry` keys it on `get_tags_for_sha(id, sha)`. An
//! unconfigured SHA yields *no* tags rather than falling back to the action's tag
//! list — a test that forgets to configure gets a visibly wrong result instead of a
//! plausible-looking pass.

use super::{Error as ResolutionError, Forge, ShaDescription, VersionRegistry};
use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Version};
use crate::domain::action::resolved::Commit;
use crate::domain::action::uses_ref::RefType;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash as _, Hasher as _};

/// The commit date every method reports unless [`FakeRegistry::with_empty_dates`]
/// is set.
const DEFAULT_DATE: &str = "2026-01-01T00:00:00Z";

/// In-memory [`VersionRegistry`] whose behavior is entirely configuration.
///
/// Construct with [`FakeRegistry::new`] and layer on builder calls. The default
/// fake never errors and reports no tags, so no test inherits a failure path it did
/// not ask for.
#[derive(Clone, Default)]
pub struct FakeRegistry {
    /// Tags per action, answering `all_tags`.
    tags: HashMap<String, Vec<String>>,
    /// Tags per `(action, sha)` pair, answering `describe_sha`.
    sha_tags: HashMap<(String, String), Vec<String>>,
    /// When set, `lookup_sha` returns this SHA for every action.
    fixed_sha: Option<String>,
    /// When set, `lookup_sha` returns this canned result verbatim.
    lookup_result: Option<Result<Commit, ResolutionError>>,
    /// When set, every method fails with this error.
    error: Option<ResolutionError>,
    /// Errors scoped to a single action, leaving other actions resolvable.
    action_errors: HashMap<String, ResolutionError>,
    /// When set, `describe_sha` fails with this error.
    describe_error: Option<ResolutionError>,
    /// When true, every reported commit date is empty.
    empty_dates: bool,
}

impl FakeRegistry {
    /// A fake that never errors and knows no tags.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tags `all_tags` reports for an action.
    ///
    /// This does *not* feed `describe_sha` — use [`Self::with_sha_tags`] for that,
    /// as production answers the two questions from different endpoints.
    #[must_use]
    pub fn with_all_tags(mut self, id: &str, tags: Vec<&str>) -> Self {
        self.tags
            .insert(id.to_owned(), tags.into_iter().map(String::from).collect());
        self
    }

    /// Register the tags that point at `sha`, answering `describe_sha`.
    #[must_use]
    pub fn with_sha_tags(mut self, id: &str, sha: &str, tags: Vec<&str>) -> Self {
        self.sha_tags.insert(
            (id.to_owned(), sha.to_owned()),
            tags.into_iter().map(String::from).collect(),
        );
        self
    }

    /// Make `lookup_sha` return `sha` for every action.
    #[must_use]
    pub fn with_fixed_sha(mut self, sha: &str) -> Self {
        self.fixed_sha = Some(sha.to_owned());
        self
    }

    /// Make `lookup_sha` return `result` verbatim.
    #[must_use]
    pub fn with_lookup_result(mut self, result: Result<Commit, ResolutionError>) -> Self {
        self.lookup_result = Some(result);
        self
    }

    /// Make every method fail with `error`.
    #[must_use]
    pub fn failing(mut self, error: ResolutionError) -> Self {
        self.error = Some(error);
        self
    }

    /// Make every method fail with `AuthRequired`, the common "no token" case.
    #[must_use]
    pub fn failing_auth(self) -> Self {
        self.failing(ResolutionError::AuthRequired {
            forge: Forge::GitHub,
        })
    }

    /// Make only `id` fail, leaving every other action resolvable.
    #[must_use]
    pub fn failing_action(mut self, id: &str, error: ResolutionError) -> Self {
        self.action_errors.insert(id.to_owned(), error);
        self
    }

    /// Make `describe_sha` fail with `error`, while the other methods still answer.
    #[must_use]
    pub fn failing_describe(mut self, error: ResolutionError) -> Self {
        self.describe_error = Some(error);
        self
    }

    /// Report an empty commit date everywhere, as a failed date fetch does.
    #[must_use]
    pub fn with_empty_dates(mut self) -> Self {
        self.empty_dates = true;
        self
    }

    /// A deterministic, SHA-shaped (40 hex chars) stand-in for a real commit SHA.
    #[must_use]
    #[expect(
        clippy::string_slice,
        reason = "slices a 48-char hex string built here, so the 40-byte boundary is always valid"
    )]
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
        full[..40].to_owned()
    }

    /// The date this fake reports, honoring [`Self::with_empty_dates`].
    fn date(&self) -> CommitDate {
        CommitDate::from(if self.empty_dates { "" } else { DEFAULT_DATE })
    }

    /// The error that applies to `id`, whether registry-wide or action-scoped.
    fn error_for(&self, id: &ActionId) -> Option<ResolutionError> {
        self.error
            .clone()
            .or_else(|| self.action_errors.get(id.as_str()).cloned())
    }
}

impl VersionRegistry for FakeRegistry {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<Commit, ResolutionError> {
        if let Some(result) = self.lookup_result.clone() {
            return result;
        }
        if let Some(error) = self.error_for(id) {
            return Err(error);
        }
        let sha = self
            .fixed_sha
            .clone()
            .unwrap_or_else(|| Self::fake_sha(id.as_str(), version.as_str()));
        Ok(Commit {
            sha: CommitSha::from(sha),
            repository: id.base_repo(),
            ref_type: Some(RefType::Tag),
            date: self.date(),
        })
    }

    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        if let Some(error) = self.error_for(id) {
            return Err(error);
        }
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
        if let Some(error) = self.describe_error.clone() {
            return Err(error);
        }
        if let Some(error) = self.error_for(id) {
            return Err(error);
        }
        // Keyed on the SHA, as production is: an unconfigured SHA has no tags.
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
            date: self.date(),
        })
    }
}

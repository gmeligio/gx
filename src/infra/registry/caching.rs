use crate::domain::action::identity::{ActionId, CommitSha, Version};
use crate::domain::action::resolved::Commit;
use crate::domain::resolution::{Error as ResolutionError, ShaDescription, VersionRegistry};
use elsa::FrozenMap;

/// Memoizes every [`VersionRegistry`] query for the lifetime of one command run.
///
/// A repository that references the same action from several workflows would
/// otherwise re-issue identical lookups, spending GitHub API quota that the
/// unauthenticated 60 requests/hour limit makes scarce. Wrapping the concrete
/// registry at the composition root deduplicates all four queries without any
/// call site having to know a cache exists.
///
/// Entries are inserted, never evicted or overwritten, and the whole map dies
/// with the process — so a value can never be stale relative to the run that
/// fetched it, and the next run always observes current registry state.
///
/// [`FrozenMap`] is what lets this work behind `&self`: it permits insertion
/// through a shared reference, so unlike a `RefCell` there is no borrow to
/// accidentally hold across the inner registry call that populates it.
pub struct Caching<R: VersionRegistry> {
    /// The registry that answers whatever this decorator has not already cached.
    inner: R,
    /// Commits by action and version, from [`VersionRegistry::lookup_sha`].
    commits: FrozenMap<(ActionId, Version), Box<Commit>>,
    /// Tags pointing at a SHA, from [`VersionRegistry::tags_for_sha`].
    /// `Vec` is already `StableDeref`, so it needs no `Box`; the map yields `&[Version]`.
    sha_tags: FrozenMap<(ActionId, CommitSha), Vec<Version>>,
    /// Every tag for an action, from [`VersionRegistry::all_tags`].
    repo_tags: FrozenMap<ActionId, Vec<Version>>,
    /// SHA descriptions, from [`VersionRegistry::describe_sha`].
    descriptions: FrozenMap<(ActionId, CommitSha), Box<ShaDescription>>,
}

impl<R: VersionRegistry> Caching<R> {
    /// Wrap `inner` so its results are reused within this run.
    #[must_use]
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            commits: FrozenMap::new(),
            sha_tags: FrozenMap::new(),
            repo_tags: FrozenMap::new(),
            descriptions: FrozenMap::new(),
        }
    }
}

// Every method follows the same shape: return the memoized value if present,
// otherwise ask the inner registry and store only on success. An `Err` is
// propagated without being recorded, so a transient failure — a rate limit, a
// missing token — never poisons the rest of the run for that key.
impl<R: VersionRegistry> VersionRegistry for Caching<R> {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<Commit, ResolutionError> {
        let key = (id.clone(), version.clone());
        if let Some(hit) = self.commits.get(&key) {
            return Ok(hit.clone());
        }
        let commit = self.inner.lookup_sha(id, version)?;
        Ok(self.commits.insert(key, Box::new(commit)).clone())
    }

    fn tags_for_sha(
        &self,
        id: &ActionId,
        sha: &CommitSha,
    ) -> Result<Vec<Version>, ResolutionError> {
        let key = (id.clone(), sha.clone());
        if let Some(hit) = self.sha_tags.get(&key) {
            return Ok(hit.to_vec());
        }
        let tags = self.inner.tags_for_sha(id, sha)?;
        Ok(self.sha_tags.insert(key, tags).to_vec())
    }

    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        if let Some(hit) = self.repo_tags.get(id) {
            return Ok(hit.to_vec());
        }
        let tags = self.inner.all_tags(id)?;
        Ok(self.repo_tags.insert(id.clone(), tags).to_vec())
    }

    fn describe_sha(
        &self,
        id: &ActionId,
        sha: &CommitSha,
    ) -> Result<ShaDescription, ResolutionError> {
        let key = (id.clone(), sha.clone());
        if let Some(hit) = self.descriptions.get(&key) {
            return Ok(hit.clone());
        }
        let description = self.inner.describe_sha(id, sha)?;
        Ok(self.descriptions.insert(key, Box::new(description)).clone())
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::assertions_on_result_states,
    clippy::arithmetic_side_effects,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
#[path = "caching_tests.rs"]
mod tests;

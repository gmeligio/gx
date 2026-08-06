use super::{Caching, VersionRegistry};
use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use crate::domain::action::resolved::Commit;
use crate::domain::resolution::{Error as ResolutionError, ShaDescription};
use std::cell::Cell;

/// Registry double that counts how many calls reach it, so a test can assert
/// the decorator absorbed the repeats. `fail` flips every method to an error.
struct CountingRegistry {
    /// Calls that reached `lookup_sha`.
    lookup_sha: Cell<usize>,
    /// Calls that reached `tags_for_sha`.
    tags_for_sha: Cell<usize>,
    /// Calls that reached `all_tags`.
    all_tags: Cell<usize>,
    /// Calls that reached `describe_sha`.
    describe_sha: Cell<usize>,
    /// When set, every method returns `RateLimited` instead of a value.
    fail: bool,
}

impl CountingRegistry {
    fn new() -> Self {
        Self {
            lookup_sha: Cell::new(0),
            tags_for_sha: Cell::new(0),
            all_tags: Cell::new(0),
            describe_sha: Cell::new(0),
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            fail: true,
            ..Self::new()
        }
    }
}

/// The commit a `CountingRegistry` reports, made distinguishable by `version`
/// so a key-collision test can tell two cached entries apart.
fn commit_for(version: &Version) -> Commit {
    Commit {
        sha: CommitSha::from(format!("sha-for-{}", version.as_str())),
        repository: Repository::from("owner/repo"),
        ref_type: None,
        date: CommitDate::from("2026-01-01T00:00:00Z"),
    }
}

impl VersionRegistry for CountingRegistry {
    fn lookup_sha(&self, _id: &ActionId, version: &Version) -> Result<Commit, ResolutionError> {
        self.lookup_sha.set(self.lookup_sha.get() + 1);
        if self.fail {
            return Err(ResolutionError::RateLimited);
        }
        Ok(commit_for(version))
    }

    fn tags_for_sha(
        &self,
        _id: &ActionId,
        sha: &CommitSha,
    ) -> Result<Vec<Version>, ResolutionError> {
        self.tags_for_sha.set(self.tags_for_sha.get() + 1);
        if self.fail {
            return Err(ResolutionError::RateLimited);
        }
        Ok(vec![Version::from(sha.as_str())])
    }

    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        self.all_tags.set(self.all_tags.get() + 1);
        if self.fail {
            return Err(ResolutionError::RateLimited);
        }
        Ok(vec![Version::from(id.as_str())])
    }

    fn describe_sha(
        &self,
        _id: &ActionId,
        sha: &CommitSha,
    ) -> Result<ShaDescription, ResolutionError> {
        self.describe_sha.set(self.describe_sha.get() + 1);
        if self.fail {
            return Err(ResolutionError::RateLimited);
        }
        Ok(ShaDescription {
            tags: vec![Version::from(sha.as_str())],
            repository: Repository::from("owner/repo"),
            date: CommitDate::from("2026-01-01T00:00:00Z"),
        })
    }
}

fn action() -> ActionId {
    ActionId::from("actions/checkout")
}

fn sha() -> CommitSha {
    CommitSha::from("abc123def456789012345678901234567890abcd")
}

#[test]
fn lookup_sha_hits_the_registry_once() {
    let inner = CountingRegistry::new();
    let cache = Caching::new(inner);
    let version = Version::from("v4");

    let first = cache.lookup_sha(&action(), &version).unwrap();
    let second = cache.lookup_sha(&action(), &version).unwrap();

    assert_eq!(first, second, "a cache hit must return the first result");
    assert_eq!(
        cache.inner.lookup_sha.get(),
        1,
        "the second lookup must be served from cache"
    );
}

#[test]
fn tags_for_sha_hits_the_registry_once() {
    let cache = Caching::new(CountingRegistry::new());

    let first = cache.tags_for_sha(&action(), &sha()).unwrap();
    let second = cache.tags_for_sha(&action(), &sha()).unwrap();

    assert_eq!(first, second);
    assert_eq!(cache.inner.tags_for_sha.get(), 1);
}

#[test]
fn all_tags_hits_the_registry_once() {
    let cache = Caching::new(CountingRegistry::new());

    let first = cache.all_tags(&action()).unwrap();
    let second = cache.all_tags(&action()).unwrap();

    assert_eq!(first, second);
    assert_eq!(
        cache.inner.all_tags.get(),
        1,
        "all_tags is called once per manifest spec in the upgrade loop, so it \
         is the method this decorator exists for"
    );
}

#[test]
fn describe_sha_hits_the_registry_once() {
    let cache = Caching::new(CountingRegistry::new());

    let first = cache.describe_sha(&action(), &sha()).unwrap();
    let second = cache.describe_sha(&action(), &sha()).unwrap();

    assert_eq!(first.tags, second.tags);
    assert_eq!(cache.inner.describe_sha.get(), 1);
}

#[test]
fn distinct_versions_are_not_conflated() {
    // The failure this guards against is silent and corrupting: a key built
    // from the action alone would serve v3's commit for a v4 lookup and write
    // the wrong SHA into gx.lock with no error anywhere.
    let cache = Caching::new(CountingRegistry::new());

    let v4 = cache.lookup_sha(&action(), &Version::from("v4")).unwrap();
    let v3 = cache.lookup_sha(&action(), &Version::from("v3")).unwrap();

    assert_eq!(cache.inner.lookup_sha.get(), 2, "distinct keys each miss");
    assert_ne!(v4.sha, v3.sha, "each version keeps its own commit");
}

#[test]
fn distinct_actions_are_not_conflated() {
    let cache = Caching::new(CountingRegistry::new());

    let checkout = cache.all_tags(&ActionId::from("actions/checkout")).unwrap();
    let cache_action = cache.all_tags(&ActionId::from("actions/cache")).unwrap();

    assert_eq!(cache.inner.all_tags.get(), 2);
    assert_ne!(checkout, cache_action);
}

#[test]
fn errors_are_not_cached() {
    // Recording a failure would turn one rate-limited request into a run-wide
    // failure for that key — strictly worse than not caching at all.
    let cache = Caching::new(CountingRegistry::failing());
    let version = Version::from("v4");

    assert!(cache.lookup_sha(&action(), &version).is_err());
    assert!(cache.lookup_sha(&action(), &version).is_err());
    assert_eq!(
        cache.inner.lookup_sha.get(),
        2,
        "a failed lookup must be retried, not replayed from cache"
    );

    assert!(cache.all_tags(&action()).is_err());
    assert!(cache.all_tags(&action()).is_err());
    assert_eq!(cache.inner.all_tags.get(), 2);

    assert!(cache.describe_sha(&action(), &sha()).is_err());
    assert!(cache.describe_sha(&action(), &sha()).is_err());
    assert_eq!(cache.inner.describe_sha.get(), 2);

    assert!(cache.tags_for_sha(&action(), &sha()).is_err());
    assert!(cache.tags_for_sha(&action(), &sha()).is_err());
    assert_eq!(cache.inner.tags_for_sha.get(), 2);
}

use super::{Retrying, Waiter};
use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use crate::domain::action::resolved::Commit;
use crate::domain::action::spec::Spec as ActionSpec;
use crate::domain::action::specifier::Specifier;
use crate::domain::resolution::{
    Error as ResolutionError, Forge, RetryAfter, ShaDescription, VersionRegistry,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// A [`Waiter`] that records requested waits and returns instantly.
#[derive(Default)]
struct RecordingWaiter {
    /// Every duration `wait` was called with, in order.
    waits: RefCell<Vec<Duration>>,
}

impl Waiter for RecordingWaiter {
    fn wait(&self, duration: Duration) {
        self.waits.borrow_mut().push(duration);
    }
}

/// A registry replaying a scripted sequence of results and counting calls.
struct ScriptedRegistry {
    /// Results to return, one per call; the last repeats once exhausted.
    script: Vec<Result<Commit, ResolutionError>>,
    /// How many times any trait method has been invoked.
    calls: RefCell<usize>,
}

impl ScriptedRegistry {
    fn new(script: Vec<Result<Commit, ResolutionError>>) -> Self {
        Self {
            script,
            calls: RefCell::new(0),
        }
    }

    fn calls(&self) -> usize {
        *self.calls.borrow()
    }

    /// Return the scripted result for this call, repeating the last one forever.
    fn next(&self) -> Result<Commit, ResolutionError> {
        let index = *self.calls.borrow();
        *self.calls.borrow_mut() += 1;
        let last = self.script.len().saturating_sub(1);
        self.script[index.min(last)].clone()
    }
}

impl VersionRegistry for ScriptedRegistry {
    fn lookup_sha(&self, _id: &ActionId, _version: &Version) -> Result<Commit, ResolutionError> {
        self.next()
    }

    fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        self.next().map(|_| Vec::new())
    }

    fn describe_sha(
        &self,
        _id: &ActionId,
        _sha: &CommitSha,
    ) -> Result<ShaDescription, ResolutionError> {
        let commit = self.next()?;
        Ok(ShaDescription {
            tags: Vec::new(),
            repository: commit.repository,
            date: commit.date,
        })
    }
}

fn commit() -> Commit {
    Commit {
        sha: CommitSha::from("abc123def456789012345678901234567890abcd"),
        repository: Repository::from("actions/checkout"),
        ref_type: None,
        date: CommitDate::from("2026-01-01T00:00:00Z"),
    }
}

fn rate_limited(retry_after: RetryAfter) -> ResolutionError {
    ResolutionError::RateLimited {
        forge: Forge::GitHub,
        retry_after,
    }
}

fn id() -> ActionId {
    ActionId::from("actions/checkout")
}

fn version() -> Version {
    Version::from("v4")
}

/// Build a retrying registry over `script`, waiting into `waiter`.
fn retrying(
    script: Vec<Result<Commit, ResolutionError>>,
    waiter: &RecordingWaiter,
) -> Retrying<ScriptedRegistry, &RecordingWaiter> {
    Retrying::new(ScriptedRegistry::new(script)).with_waiter(waiter)
}

impl Waiter for &RecordingWaiter {
    fn wait(&self, duration: Duration) {
        (*self).wait(duration);
    }
}

#[test]
fn transient_rate_limit_resolves_without_user_intervention() {
    let waiter = RecordingWaiter::default();
    let registry = retrying(
        vec![Err(rate_limited(RetryAfter::Unstated)), Ok(commit())],
        &waiter,
    );

    let result = registry.lookup_sha(&id(), &version());

    assert!(result.is_ok(), "the retry recovers the transient limit");
    assert_eq!(
        registry.inner.calls(),
        2,
        "one failed attempt plus one successful retry"
    );
}

#[test]
fn persistent_rate_limit_fails_after_bounded_attempts() {
    let waiter = RecordingWaiter::default();
    let registry = retrying(vec![Err(rate_limited(RetryAfter::Unstated))], &waiter);

    let result = registry.lookup_sha(&id(), &version());

    assert!(result.is_err(), "an exhausted quota still fails");
    assert_eq!(
        registry.inner.calls(),
        3,
        "bounded at MAX_ATTEMPTS, and greater than 1 so retry actually happened"
    );
}

#[test]
fn missing_credential_is_not_retried() {
    let waiter = RecordingWaiter::default();
    let registry = retrying(
        vec![Err(ResolutionError::AuthRequired {
            forge: Forge::GitHub,
        })],
        &waiter,
    );

    let result = registry.lookup_sha(&id(), &version());

    result.unwrap_err();
    assert_eq!(
        registry.inner.calls(),
        1,
        "reissuing the same absent credential cannot succeed"
    );
    assert!(waiter.waits.borrow().is_empty(), "and never sleeps");
}

#[test]
fn strict_failure_is_not_retried() {
    let waiter = RecordingWaiter::default();
    let registry = retrying(
        vec![Err(ResolutionError::ResolveFailed {
            spec: ActionSpec::new(id(), Specifier::from_v1("v4")),
            reason: "not found".to_owned(),
        })],
        &waiter,
    );

    let result = registry.lookup_sha(&id(), &version());

    result.unwrap_err();
    assert_eq!(registry.inner.calls(), 1, "a 404 will not become a 200");
}

#[test]
fn stated_reset_time_is_waited_out_exactly() {
    let waiter = RecordingWaiter::default();
    let registry = retrying(
        vec![
            Err(rate_limited(RetryAfter::After(Duration::from_secs(3)))),
            Ok(commit()),
        ],
        &waiter,
    );

    let result = registry.lookup_sha(&id(), &version());

    result.unwrap();
    assert_eq!(
        *waiter.waits.borrow(),
        vec![Duration::from_secs(3)],
        "the forge's stated reset wins over the backoff schedule"
    );
}

#[test]
fn unstated_reset_falls_back_to_increasing_backoff() {
    let waiter = RecordingWaiter::default();
    let registry = retrying(vec![Err(rate_limited(RetryAfter::Unstated))], &waiter);

    let result = registry.lookup_sha(&id(), &version());

    result.unwrap_err();
    assert_eq!(
        *waiter.waits.borrow(),
        vec![Duration::from_secs(1), Duration::from_secs(2)],
        "backoff increases between attempts"
    );
}

#[test]
fn distant_reset_stops_rather_than_backing_off() {
    let waiter = RecordingWaiter::default();
    let registry = retrying(vec![Err(rate_limited(RetryAfter::TooDistant))], &waiter);

    let result = registry.lookup_sha(&id(), &version());

    assert!(result.is_err(), "the user gets their warning now");
    // Contrast with `unstated_reset_falls_back_to_increasing_backoff`, which is
    // identical but for the RetryAfter: that one retries twice and sleeps 3s.
    // Together they prove TooDistant is what suppresses the retry, not inertia.
    assert_eq!(registry.inner.calls(), 1, "no retry on an hour-out reset");
    assert!(
        waiter.waits.borrow().is_empty(),
        "and above all, no stalled terminal"
    );
}

#[test]
fn each_wait_is_announced_before_it_happens() {
    // `Rc` rather than a borrow: the notifier is `'static`, mirroring how a
    // command shares a sink with a registry that outlives the enclosing scope.
    let announced = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&announced);
    let waiter = RecordingWaiter::default();
    let registry = Retrying::new(ScriptedRegistry::new(vec![
        Err(rate_limited(RetryAfter::After(Duration::from_secs(2)))),
        Ok(commit()),
    ]))
    .with_notifier(move |message| sink.borrow_mut().push(message.to_owned()))
    .with_waiter(&waiter);

    let result = registry.lookup_sha(&id(), &version());

    result.unwrap();
    let messages = announced.borrow();
    assert_eq!(messages.len(), 1, "one notice per wait");
    assert!(
        messages[0].contains("rate limit"),
        "names the cause: {}",
        messages[0]
    );
    assert!(
        messages[0].contains('2'),
        "names the duration: {}",
        messages[0]
    );
}

#[test]
fn every_trait_method_retries_not_just_lookup_sha() {
    let waiter = RecordingWaiter::default();
    let tags = retrying(
        vec![Err(rate_limited(RetryAfter::Unstated)), Ok(commit())],
        &waiter,
    );
    tags.all_tags(&id()).unwrap();
    assert_eq!(tags.inner.calls(), 2, "all_tags routes through the loop");

    let describe = retrying(
        vec![Err(rate_limited(RetryAfter::Unstated)), Ok(commit())],
        &waiter,
    );
    let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
    describe.describe_sha(&id(), &sha).unwrap();
    assert_eq!(
        describe.inner.calls(),
        2,
        "describe_sha routes through the loop"
    );
}

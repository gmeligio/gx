use super::{Retrying, Waiter};
use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use crate::domain::action::resolved::Commit;
use crate::domain::action::spec::Spec as ActionSpec;
use crate::domain::action::specifier::Specifier;
use crate::domain::resolution::{
    Error as ResolutionError, Forge, MAX_RETRY_WAIT_SECS, RetryAfter, ShaDescription,
    VersionRegistry,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// One observable act of the retry loop, in the order it happened.
///
/// Waits and announcements land in a single sequence rather than two collections,
/// because the ordering between them is itself a requirement: an announcement
/// after its wait leaves the pause unexplained while it is happening, which is the
/// stall the notifier exists to prevent. Two separate logs cannot see that
/// inversion — each stays correct on its own while the interleaving is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    /// The loop announced a pending wait, carrying the message it emitted.
    Announced(String),
    /// The loop slept for this duration.
    Waited(Duration),
}

/// Records what the retry loop did, in order, and returns from waits instantly.
///
/// Instant returns let a test assert the wait *schedule* instead of enduring it —
/// a real sleep could only prove the schedule by being slow.
#[derive(Default)]
struct Timeline {
    /// Every wait and announcement, interleaved in occurrence order.
    events: RefCell<Vec<Event>>,
}

impl Timeline {
    /// Durations of the waits taken, in order, dropping announcements.
    fn waits(&self) -> Vec<Duration> {
        self.events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                &Event::Waited(duration) => Some(duration),
                Event::Announced(_) => None,
            })
            .collect()
    }

    /// Messages announced, in order, dropping waits.
    fn announcements(&self) -> Vec<String> {
        self.events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                Event::Announced(message) => Some(message.clone()),
                Event::Waited(_) => None,
            })
            .collect()
    }

    /// Record an announcement. Shared with the notifier the registry is built with.
    fn announce(&self, message: &str) {
        self.events
            .borrow_mut()
            .push(Event::Announced(message.to_owned()));
    }
}

impl Waiter for &Timeline {
    fn wait(&self, duration: Duration) {
        self.events.borrow_mut().push(Event::Waited(duration));
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

/// Build a retrying registry over `script`, recording into `timeline`.
fn retrying(
    script: Vec<Result<Commit, ResolutionError>>,
    timeline: &Timeline,
) -> Retrying<'_, ScriptedRegistry, &Timeline> {
    Retrying::new(ScriptedRegistry::new(script)).with_waiter(timeline)
}

/// The wait schedule an exhausted budget produces when nothing overrides it.
///
/// Named because four scenarios below converge on it from different inputs — an
/// unstated reset, a zero reset, a restated reset, and a capped one — and the
/// point of each is that it lands here.
fn backoff_schedule() -> Vec<Duration> {
    vec![Duration::from_secs(1), Duration::from_secs(2)]
}

// ---------------------------------------------------------------------------
// Requirement: rate-limited resolution is retried within a bounded budget
// ---------------------------------------------------------------------------

#[test]
fn transient_rate_limit_resolves_without_user_intervention() {
    let timeline = Timeline::default();
    let registry = retrying(
        vec![Err(rate_limited(RetryAfter::Unstated)), Ok(commit())],
        &timeline,
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
    let timeline = Timeline::default();
    let registry = retrying(vec![Err(rate_limited(RetryAfter::Unstated))], &timeline);

    let result = registry.lookup_sha(&id(), &version());

    assert!(result.is_err(), "an exhausted quota still fails");
    assert_eq!(
        registry.inner.calls(),
        3,
        "bounded by the BACKOFF schedule, and greater than 1 so retry actually happened"
    );
}

/// Both non-retryable classifications, which differ only in why they stop.
///
/// One table rather than two near-identical tests: the assertion is the same —
/// one call, no wait — and a new non-retryable variant should extend the table
/// rather than copy a test body.
#[test]
fn a_non_retryable_failure_is_returned_on_the_first_attempt() {
    let cases = [
        (
            ResolutionError::AuthRequired {
                forge: Forge::GitHub,
            },
            "reissuing the same absent credential cannot succeed",
        ),
        (
            ResolutionError::ResolveFailed {
                spec: ActionSpec::new(id(), Specifier::from_v1("v4")),
                reason: "not found".to_owned(),
            },
            "a 404 will not become a 200",
        ),
    ];

    for (error, why) in cases {
        let timeline = Timeline::default();
        let registry = retrying(vec![Err(error)], &timeline);

        registry.lookup_sha(&id(), &version()).unwrap_err();

        assert_eq!(registry.inner.calls(), 1, "{why}");
        assert!(timeline.waits().is_empty(), "and never sleeps: {why}");
    }
}

#[test]
fn every_trait_method_retries_not_just_lookup_sha() {
    let timeline = Timeline::default();
    let tags = retrying(
        vec![Err(rate_limited(RetryAfter::Unstated)), Ok(commit())],
        &timeline,
    );
    tags.all_tags(&id()).unwrap();
    assert_eq!(tags.inner.calls(), 2, "all_tags routes through the loop");

    let describe = retrying(
        vec![Err(rate_limited(RetryAfter::Unstated)), Ok(commit())],
        &timeline,
    );
    let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
    describe.describe_sha(&id(), &sha).unwrap();
    assert_eq!(
        describe.inner.calls(),
        2,
        "describe_sha routes through the loop"
    );
}

// ---------------------------------------------------------------------------
// Requirement: the wait honors the forge's reset time but is capped
// ---------------------------------------------------------------------------

#[test]
fn a_stated_reset_above_the_backoff_step_is_waited_out() {
    let timeline = Timeline::default();
    let registry = retrying(
        vec![
            Err(rate_limited(RetryAfter::After(Duration::from_secs(3)))),
            Ok(commit()),
        ],
        &timeline,
    );

    registry.lookup_sha(&id(), &version()).unwrap();

    assert_eq!(
        timeline.waits(),
        vec![Duration::from_secs(3)],
        "a stated reset above the backoff step wins over the schedule"
    );
}

/// Every stated reset that must not be taken at face value, and why.
///
/// These were three separate tests asserting the identical `[1s, 2s]` schedule.
/// They are one case each of a single rule — a stated reset is a floor clamped to
/// the cap, never the literal wait — so they belong in one table where that rule
/// is stated once and a fourth input is a row rather than another copied body.
#[test]
fn a_stated_reset_is_floored_to_the_backoff_and_clamped_to_the_cap() {
    let cap = Duration::from_secs(MAX_RETRY_WAIT_SECS);
    let cases = [
        (
            Duration::ZERO,
            backoff_schedule(),
            "a 0s reset — the forge's whole-second epoch rounding down, or a local \
             clock running ahead — is floored to the backoff, never hammering the forge",
        ),
        (
            Duration::from_secs(1),
            backoff_schedule(),
            "a reset restated every attempt escalates instead of re-waiting 1s",
        ),
        (
            Duration::from_hours(1),
            vec![cap, cap],
            "an hour-long stated reset never becomes an hour-long sleep; only a \
             second forge can state this, since the GitHub registry caps first",
        ),
    ];

    for (stated, expected, why) in cases {
        let timeline = Timeline::default();
        let registry = retrying(
            vec![Err(rate_limited(RetryAfter::After(stated)))],
            &timeline,
        );

        registry.lookup_sha(&id(), &version()).unwrap_err();

        assert_eq!(timeline.waits(), expected, "{why}");
    }
}

#[test]
fn an_unstated_reset_falls_back_to_increasing_backoff() {
    let timeline = Timeline::default();
    let registry = retrying(vec![Err(rate_limited(RetryAfter::Unstated))], &timeline);

    registry.lookup_sha(&id(), &version()).unwrap_err();

    assert_eq!(
        timeline.waits(),
        backoff_schedule(),
        "backoff increases between attempts"
    );
}

#[test]
fn a_distant_reset_stops_rather_than_backing_off() {
    let timeline = Timeline::default();
    let registry = retrying(vec![Err(rate_limited(RetryAfter::TooDistant))], &timeline);

    let result = registry.lookup_sha(&id(), &version());

    assert!(result.is_err(), "the user gets their warning now");
    // Contrast with `an_unstated_reset_falls_back_to_increasing_backoff`, which is
    // identical but for the RetryAfter: that one retries twice and sleeps 3s.
    // Together they prove TooDistant is what suppresses the retry, not inertia.
    assert_eq!(registry.inner.calls(), 1, "no retry on an hour-out reset");
    assert!(
        timeline.waits().is_empty(),
        "and above all, no stalled terminal"
    );
}

// ---------------------------------------------------------------------------
// Requirement: a retry wait is announced to the user
// ---------------------------------------------------------------------------

/// The announcement names the cause and the duration, and precedes its wait.
///
/// The ordering half is asserted against the interleaved timeline, not against a
/// separate log of announcements: with two logs, moving the sleep ahead of the
/// notify call leaves both individually correct and the stall undetected. This
/// asserts the whole event sequence, so that inversion fails the test.
#[test]
fn each_wait_is_announced_before_it_happens() {
    let timeline = Rc::new(Timeline::default());
    let sink = Rc::clone(&timeline);
    let registry = Retrying::new(ScriptedRegistry::new(vec![
        Err(rate_limited(RetryAfter::After(Duration::from_secs(2)))),
        Ok(commit()),
    ]))
    .with_notifier(move |message| sink.announce(message))
    .with_waiter(&*timeline);

    registry.lookup_sha(&id(), &version()).unwrap();

    let announcements = timeline.announcements();
    assert_eq!(announcements.len(), 1, "one notice per wait");
    let notice = announcements.first().unwrap();
    assert!(notice.contains("rate limit"), "names the cause: {notice}");
    assert!(notice.contains('2'), "names the duration: {notice}");
    assert_eq!(
        *timeline.events.borrow(),
        vec![
            Event::Announced(notice.clone()),
            Event::Waited(Duration::from_secs(2))
        ],
        "the explanation lands before the pause, never after it"
    );
}

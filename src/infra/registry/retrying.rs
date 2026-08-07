//! Bounded retry with backoff for rate-limited resolution requests.

use crate::domain::action::identity::{ActionId, CommitSha, Version};
use crate::domain::action::resolved::Commit;
use crate::domain::resolution::{
    Error as ResolutionError, RetryAfter, ShaDescription, VersionRegistry,
};
use crate::infra::github::MAX_RETRY_WAIT_SECS;
use std::time::Duration;

/// Total attempts per request, counting the first.
///
/// Two retries covers a quota window that ticks over within a few seconds. With
/// the backoff below that is at most 3s of waiting; a limit that has not lifted
/// by then, and stated no reset time, is not lifting on this run's timescale.
const MAX_ATTEMPTS: usize = 3;

/// Waits used when the forge stated no reset time, indexed by retry number.
///
/// Increasing, and every value within the cap a stated reset is clamped to — so
/// an unstated reset never waits longer than a stated one would be allowed to.
/// Enforced by the assertion below rather than left to this comment.
const BACKOFF: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(2)];

// Fails the build if BACKOFF ever outgrows the cap it claims to respect. The
// two constants live in different modules — the cap beside the header parsing
// that applies it, the schedule beside the loop that uses it — so nothing but
// this binds them. Without it, raising a backoff value past the cap would leave
// the doc comment above quietly false.
// Destructured rather than indexed, so adding a backoff step is a compile error
// here until it is covered too.
const _: () = {
    let [first, second] = BACKOFF;
    assert!(
        first.as_secs() <= MAX_RETRY_WAIT_SECS && second.as_secs() <= MAX_RETRY_WAIT_SECS,
        "BACKOFF must stay within MAX_RETRY_WAIT_SECS"
    );
};

/// Pauses execution for a requested duration.
///
/// Injectable so tests can assert the wait *schedule* instead of enduring it —
/// a real sleep could only prove the schedule by being slow.
pub trait Waiter {
    /// Block for `duration`.
    fn wait(&self, duration: Duration);
}

/// The production [`Waiter`]: really sleeps.
#[derive(Debug, Default, Clone, Copy)]
pub struct ThreadWaiter;

impl Waiter for ThreadWaiter {
    fn wait(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Announces a pending wait to the user. See [`Retrying::with_notifier`].
///
/// Borrows for `'notify` rather than requiring `'static`, so a command can hand
/// over a closure sharing the progress callback it already owns on the stack.
type Notifier<'notify> = Box<dyn Fn(&str) + 'notify>;

/// Retries rate-limited requests to an inner registry, with backoff.
///
/// Only [`ResolutionError::RateLimited`] is retried, as decided by
/// [`ResolutionError::is_retryable`]. A missing credential is skippable but never
/// retryable — reissuing it cannot succeed — and every other error is returned
/// on the first attempt.
///
/// Designed as the *inner* layer of a decorator stack, so a caching layer
/// wrapping this one short-circuits on a hit before any retry runs.
pub struct Retrying<'notify, R: VersionRegistry, W: Waiter = ThreadWaiter> {
    /// The registry whose failures are retried.
    inner: R,
    /// How the retry pauses between attempts.
    waiter: W,
    /// Invoked immediately before each wait, if set.
    notifier: Option<Notifier<'notify>>,
}

impl<R: VersionRegistry> Retrying<'_, R> {
    /// Wrap a registry, sleeping for real and announcing nothing.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            waiter: ThreadWaiter,
            notifier: None,
        }
    }
}

impl<'notify, R: VersionRegistry, W: Waiter> Retrying<'notify, R, W> {
    /// Announce each wait through `notifier` before sleeping.
    ///
    /// Without this a multi-second pause is indistinguishable from a hang. The
    /// callback is `Fn`, not `FnMut`, so it can be held alongside the `&mut`
    /// progress callback the commands already pass to their planners.
    #[must_use]
    pub fn with_notifier<N: Fn(&str) + 'notify>(mut self, notifier: N) -> Self {
        self.notifier = Some(Box::new(notifier));
        self
    }

    /// Swap in a different [`Waiter`]. Used by tests to avoid real sleeping.
    #[cfg(test)]
    fn with_waiter<W2: Waiter>(self, waiter: W2) -> Retrying<'notify, R, W2> {
        Retrying {
            inner: self.inner,
            waiter,
            notifier: self.notifier,
        }
    }

    /// How long to wait before retry number `retry_index`, or `None` to stop.
    ///
    /// `TooDistant` yields `None`: the forge's quota resets further out than any
    /// wait worth taking, so backing off would only delay the same failure while
    /// holding the user's terminal.
    fn wait_for(error: &ResolutionError, retry_index: usize) -> Option<Duration> {
        // Only `RateLimited` states a reset; every other variant reaches here
        // solely because `is_retryable` let it through, which it does not.
        let ResolutionError::RateLimited { retry_after, .. } = error else {
            return None;
        };
        match retry_after {
            RetryAfter::After(duration) => Some(*duration),
            RetryAfter::Unstated => BACKOFF.get(retry_index).copied(),
            RetryAfter::TooDistant => None,
        }
    }

    /// Run `attempt` until it succeeds, stops being retryable, or runs out of budget.
    fn retrying<T>(
        &self,
        mut attempt: impl FnMut() -> Result<T, ResolutionError>,
    ) -> Result<T, ResolutionError> {
        let mut retry_index: usize = 0;
        loop {
            let error = match attempt() {
                Ok(value) => return Ok(value),
                Err(error) => error,
            };

            if !error.is_retryable() || retry_index.saturating_add(1) >= MAX_ATTEMPTS {
                return Err(error);
            }
            let Some(duration) = Self::wait_for(&error, retry_index) else {
                return Err(error);
            };

            if let Some(notify) = self.notifier.as_ref() {
                notify(&format!(
                    "{error}; retrying in {}s",
                    duration.as_secs_f32().ceil()
                ));
            }
            self.waiter.wait(duration);
            retry_index = retry_index.saturating_add(1);
        }
    }
}

impl<R: VersionRegistry, W: Waiter> VersionRegistry for Retrying<'_, R, W> {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<Commit, ResolutionError> {
        self.retrying(|| self.inner.lookup_sha(id, version))
    }

    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        self.retrying(|| self.inner.all_tags(id))
    }

    fn describe_sha(
        &self,
        id: &ActionId,
        sha: &CommitSha,
    ) -> Result<ShaDescription, ResolutionError> {
        self.retrying(|| self.inner.describe_sha(id, sha))
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
#[path = "retrying_tests.rs"]
mod tests;

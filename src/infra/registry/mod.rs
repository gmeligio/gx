//! Forge-agnostic decorators over [`crate::domain::resolution::VersionRegistry`].
//!
//! These wrap any registry rather than any particular forge: they key off the
//! forge-neutral [`crate::domain::resolution::Error`] classification, so a second
//! backend gets the same behavior for free.
//!
//! Composed cache-outside-retry — `Caching::new(Retrying::new(registry))` — so a
//! cache hit short-circuits before any retry runs, and a wait is only ever spent
//! on a request that genuinely has to reach the forge.

#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// Per-run memoizing decorator over any [`crate::domain::resolution::VersionRegistry`].
mod caching;
/// Bounded retry with backoff for rate-limited resolution requests.
mod retrying;

pub use caching::Caching;
pub use retrying::{Retrying, Waiter};

use crate::domain::resolution::VersionRegistry;
use std::cell::RefCell;
use std::rc::Rc;

/// A progress callback shared between a command and its registry.
///
/// Commands own their `&mut dyn FnMut(&str)` and hand it to a planner, so a
/// registry cannot also hold it. Moving it behind `Rc<RefCell<_>>` lets both
/// reach it: the planner borrows it per call, and the retry layer borrows it to
/// announce a wait *before* sleeping, so the spinner explains the pause while
/// the pause is happening rather than after it.
pub type SharedProgress<'cb> = Rc<RefCell<&'cb mut dyn FnMut(&str)>>;

/// Build the decorator stack every command wraps its forge registry in.
///
/// Cache outside retry: a repeated query is answered from the cache and never
/// reaches the retry layer, so a wait is only ever spent on a request that
/// genuinely has to reach the forge. Inverting this would let a cached answer
/// sit behind a sleep that had already been paid for an earlier identical call.
///
/// The returned registry and the returned progress closure share `on_progress`.
/// Pass the closure to the planner in place of the original callback.
pub fn caching_retrying<'cb, R: VersionRegistry>(
    inner: R,
    on_progress: &'cb mut dyn FnMut(&str),
) -> (Caching<Retrying<'cb, R>>, impl FnMut(&str) + use<'cb, R>) {
    let shared: SharedProgress<'cb> = Rc::new(RefCell::new(on_progress));
    let announce = Rc::clone(&shared);
    let registry = Retrying::new(inner).with_notifier(move |message| {
        // The planner never holds this borrow across a registry call, so the
        // borrow is always free when a retry announces.
        if let Ok(mut emit) = announce.try_borrow_mut() {
            emit(message);
        }
    });
    let progress = move |message: &str| {
        if let Ok(mut emit) = shared.try_borrow_mut() {
            emit(message);
        }
    };
    (Caching::new(registry), progress)
}

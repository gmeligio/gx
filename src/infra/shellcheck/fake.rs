//! A [`ShellChecker`] test double. Returns pre-seeded [`Finding`]s without spawning any
//! process, so the `run-shellcheck` rule's logic can be unit-tested with no binary on
//! `PATH` and with fully deterministic findings.

use super::{Finding, Sh, ShellChecker};
use std::cell::RefCell;

/// Test double that returns canned findings and records the scripts it was asked to check.
pub struct FakeChecker {
    /// Findings returned from every `check` call.
    findings: Vec<Finding>,
    /// Scripts passed to `check`, in call order, for assertions on sanitization/offsets.
    pub seen: RefCell<Vec<String>>,
}

impl FakeChecker {
    /// A checker that returns `findings` for every call.
    #[must_use]
    pub fn new(findings: Vec<Finding>) -> Self {
        Self {
            findings,
            seen: RefCell::new(Vec::new()),
        }
    }

    /// A checker that always reports a clean script.
    #[must_use]
    pub fn clean() -> Self {
        Self::new(Vec::new())
    }
}

impl ShellChecker for FakeChecker {
    fn check(&self, script: &str, _shell: Sh) -> Vec<Finding> {
        self.seen.borrow_mut().push(script.to_owned());
        self.findings.clone()
    }
}

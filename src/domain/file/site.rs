//! Addressing for managed references: which file, and where within it.
//!
//! This module is a leaf — it imports nothing else from `crate::domain`. That is
//! deliberate: both the manifest's override addressing and the lint layer's ignore
//! targets need these types, and a leaf can be depended on from either without the
//! two needing to know about each other.

/// A workflow file path with forward-slash normalization.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkflowPath(String);

impl WorkflowPath {
    pub fn new<S: Into<String>>(path: S) -> Self {
        Self(path.into().replace('\\', "/"))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WorkflowPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A workflow job identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JobId(String);

impl JobId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for JobId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for JobId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// A 0-based step index within a workflow job.
///
/// Wraps `u16` to make `From<StepIndex> for i64` infallible,
/// eliminating `expect("step index overflow")` in TOML serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StepIndex(u16);

impl StepIndex {
    /// Returns the raw `u16` value.
    #[must_use]
    pub fn as_u16(self) -> u16 {
        self.0
    }
}

impl From<u16> for StepIndex {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl From<StepIndex> for i64 {
    fn from(value: StepIndex) -> Self {
        Self::from(value.0)
    }
}

impl TryFrom<i64> for StepIndex {
    type Error = String;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        let raw = u16::try_from(value)
            .map_err(|_| format!("invalid step index: {value} (must be 0..=65535)"))?;
        Ok(Self(raw))
    }
}

impl TryFrom<usize> for StepIndex {
    type Error = String;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        let raw = u16::try_from(value)
            .map_err(|_| format!("invalid step index: {value} (must be 0..=65535)"))?;
        Ok(Self(raw))
    }
}

impl std::fmt::Display for StepIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Where within a file a reference sits.
///
/// Replaces an earlier `(Option<JobId>, Option<StepIndex>)` pair, two of whose four
/// representable combinations the scanner never produced. The composite case was
/// distinguished by `job.is_none() && step.is_some()`; that rule now holds by
/// construction rather than by convention, so the tiers of override resolution cannot
/// collide.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Slot {
    /// A step of a workflow job: `jobs.<job>.steps[<step>]`.
    WorkflowStep {
        /// The job the step belongs to.
        job: JobId,
        /// 0-based index within that job's steps.
        step: StepIndex,
    },
    /// A step of a composite action: `runs.steps[<step>]`. A composite action has no
    /// jobs, so there is no job id to carry — gx does not fabricate one.
    CompositeStep {
        /// 0-based index within `runs.steps`.
        step: StepIndex,
    },
    /// A job-level `uses:` — a reusable-workflow call, which has no step index.
    WorkflowJob {
        /// The job holding the `uses:`.
        job: JobId,
    },
}

impl Slot {
    /// The job this site belongs to, if its schema has jobs.
    #[must_use]
    pub fn job(&self) -> Option<&JobId> {
        match self {
            Self::WorkflowStep { job, .. } | Self::WorkflowJob { job } => Some(job),
            Self::CompositeStep { .. } => None,
        }
    }

    /// The step index within its list, if this site is a step.
    #[must_use]
    pub fn step(&self) -> Option<StepIndex> {
        match self {
            Self::WorkflowStep { step, .. } | Self::CompositeStep { step } => Some(*step),
            Self::WorkflowJob { .. } => None,
        }
    }
}

/// The identity of a reference site: which file, and where within it.
///
/// This is what user configuration addresses and what override resolution matches on.
/// It deliberately carries no provenance — see [`Origin`] — so that two references to
/// the same site compare and hash equal regardless of where they were read from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Id {
    /// Relative path from repo root, e.g. `.github/workflows/ci.yml`.
    pub file: WorkflowPath,
    /// Position within that file.
    pub slot: Slot,
}

/// Where a reference was read from, for reporting.
///
/// Separate from [`Id`] because provenance must never participate in matching: an
/// override written by hand has no line number, and would otherwise fail to match the
/// same site discovered by a parse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origin {
    /// 1-based source line of the `uses:` scalar, when known. `None` for sites
    /// synthesized outside a parse (e.g. manifest-derived entries).
    pub line: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::{Id, JobId, Origin, Slot, StepIndex, WorkflowPath};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher as _};

    fn hash_of<T: Hash>(value: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    fn workflow_site() -> Id {
        Id {
            file: WorkflowPath::new(".github/workflows/ci.yml"),
            slot: Slot::WorkflowStep {
                job: JobId::from("build"),
                step: StepIndex::from(0),
            },
        }
    }

    /// The invariant the previous `Location` could not state: identity is independent of
    /// provenance. `Location` derived `Eq` but not `Hash` precisely because its `line`
    /// field would have poisoned it.
    #[test]
    fn identity_is_independent_of_origin() {
        let (a, b) = (workflow_site(), workflow_site());
        let (origin_a, origin_b) = (Origin { line: Some(12) }, Origin { line: Some(40) });

        assert_eq!(a, b, "same site must compare equal");
        assert_eq!(hash_of(&a), hash_of(&b), "same site must hash equal");
        assert_ne!(
            origin_a, origin_b,
            "differing provenance is still observable, just not part of identity"
        );
    }

    #[test]
    fn sites_in_different_files_are_distinct() {
        let mut other = workflow_site();
        other.file = WorkflowPath::new(".github/workflows/release.yml");
        assert_ne!(workflow_site(), other);
    }

    /// A composite step and a workflow step at the same index are different addresses.
    /// Under the previous `Option` pair both were `step: Some(0)`, distinguished only by
    /// `job` being `None`.
    #[test]
    fn composite_step_is_distinct_from_workflow_step() {
        let composite = Id {
            file: WorkflowPath::new(".github/actions/setup/action.yml"),
            slot: Slot::CompositeStep {
                step: StepIndex::from(0),
            },
        };
        let workflow = Id {
            file: WorkflowPath::new(".github/actions/setup/action.yml"),
            slot: Slot::WorkflowStep {
                job: JobId::from("build"),
                step: StepIndex::from(0),
            },
        };
        assert_ne!(composite, workflow);
    }

    #[test]
    fn job_level_uses_has_no_step() {
        let job_level = Slot::WorkflowJob {
            job: JobId::from("release"),
        };
        assert_ne!(
            job_level,
            Slot::WorkflowStep {
                job: JobId::from("release"),
                step: StepIndex::from(0),
            }
        );
    }

    #[test]
    fn origin_defaults_to_no_line() {
        assert_eq!(Origin::default().line, None);
    }
}

use super::action::identity::{ActionId, Version};
use super::action::uses_ref::InterpretedRef;
use std::collections::{HashMap, HashSet};

/// Aggregates action versions discovered across all workflows.
/// This handles the domain logic of deciding which version "wins"
/// when multiple versions exist for the same action.
#[derive(Debug, Default)]
pub struct ActionSet {
    /// Maps action ID to set of versions found in workflows.
    versions: HashMap<ActionId, HashSet<Version>>,
    /// Count of how many times each version appears for each action (across all steps).
    counts: HashMap<ActionId, HashMap<Version, usize>>,
}

impl ActionSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an `ActionSet` from a slice of `Located`.
    /// Builds the `versions` and `counts` maps from the actions.
    #[must_use]
    pub fn from_located(actions: &[Located]) -> Self {
        let mut set = Self::new();
        for action in actions {
            set.add(&action.action);
        }
        set
    }

    /// Add an interpreted action reference to the set.
    pub fn add(&mut self, interpreted: &InterpretedRef) {
        self.versions
            .entry(interpreted.id.clone())
            .or_default()
            .insert(interpreted.version.clone());

        // Track occurrence count for dominant_version selection
        let count = self
            .counts
            .entry(interpreted.id.clone())
            .or_default()
            .entry(interpreted.version.clone())
            .or_insert(0);
        *count = count.saturating_add(1);
    }

    /// Select the dominant version for an action:
    /// 1. Most-used (highest occurrence count across all steps)
    /// 2. Tiebreak: highest semver
    #[must_use]
    pub fn dominant_version(&self, id: &ActionId) -> Option<Version> {
        let counts = self.counts.get(id)?;
        let max_count = counts.values().max().copied()?;
        let candidates: Vec<Version> = counts
            .iter()
            .filter(|(_, c)| **c == max_count)
            .map(|(v, _)| v.clone())
            .collect();
        Version::highest(&candidates)
    }

    /// Returns true if no actions have been added.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }

    /// Get all unique versions found for an action.
    pub fn versions_for(&self, id: &ActionId) -> impl Iterator<Item = &Version> {
        self.versions
            .get(id)
            .map(|v| v.iter())
            .into_iter()
            .flatten()
    }

    /// Get all action IDs discovered across workflows.
    pub fn action_ids(&self) -> impl Iterator<Item = &ActionId> {
        self.versions.keys()
    }
}

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

/// The precise location of a `uses:` reference within the workflow tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// Relative path from repo root, e.g. ".github/workflows/ci.yml".
    pub workflow: WorkflowPath,
    /// Job id, e.g. "build".
    pub job: Option<JobId>,
    /// 0-based step index within the job.
    pub step: Option<StepIndex>,
}

/// A single action reference with its full location context.
#[derive(Debug, Clone)]
pub struct Located {
    /// The interpreted action reference (id, version, optional SHA).
    pub action: InterpretedRef,
    pub location: Location,
}

#[cfg(test)]
mod tests {
    use super::{
        ActionId, ActionSet, InterpretedRef, JobId, Located, Location, StepIndex, Version,
        WorkflowPath,
    };
    use crate::domain::action::identity::CommitSha;

    fn make_interpreted(name: &str, version: &str, sha: Option<&str>) -> InterpretedRef {
        InterpretedRef {
            id: ActionId::from(name),
            version: Version::from(version),
            sha: sha.map(CommitSha::from),
        }
    }

    #[test]
    fn most_used_version_two_vs_one() {
        let mut set = ActionSet::new();
        // Add v3 twice (two different steps)
        set.add(&make_interpreted("actions/checkout", "v3", None));
        set.add(&make_interpreted("actions/checkout", "v3", None));
        // Add v4 once
        set.add(&make_interpreted("actions/checkout", "v4", None));

        // v3 appears 2 times, v4 appears 1 time — v3 wins even though v4 is higher semver
        let dominant = set.dominant_version(&ActionId::from("actions/checkout"));
        assert_eq!(dominant, Some(Version::from("v3")));
    }

    #[test]
    fn dominant_version_tiebreak_highest_semver() {
        let mut set = ActionSet::new();
        // Both versions appear once — tiebreak by highest semver
        set.add(&make_interpreted("actions/checkout", "v3", None));
        set.add(&make_interpreted("actions/checkout", "v4", None));

        let dominant = set.dominant_version(&ActionId::from("actions/checkout"));
        assert_eq!(dominant, Some(Version::from("v4")));
    }

    #[test]
    fn workflow_location_equality() {
        let loc1 = Location {
            workflow: WorkflowPath::new(".github/workflows/ci.yml"),
            job: Some(JobId::from("build")),
            step: Some(StepIndex::from(0_u16)),
        };
        let loc2 = Location {
            workflow: WorkflowPath::new(".github/workflows/ci.yml"),
            job: Some(JobId::from("build")),
            step: Some(StepIndex::from(0_u16)),
        };
        assert_eq!(loc1, loc2);
    }

    #[test]
    fn located_action_stores_location() {
        let loc = Location {
            workflow: WorkflowPath::new(".github/workflows/ci.yml"),
            job: Some(JobId::from("build")),
            step: Some(StepIndex::from(0_u16)),
        };
        let action = Located {
            action: InterpretedRef {
                id: ActionId::from("actions/checkout"),
                version: Version::from("v4"),
                sha: None,
            },
            location: loc.clone(),
        };
        assert_eq!(action.location, loc);
        assert_eq!(action.action.id.as_str(), "actions/checkout");
    }

    #[test]
    fn add_single_version() {
        let mut set = ActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));

        let versions: Vec<_> = set
            .versions_for(&ActionId::from("actions/checkout"))
            .collect();
        assert_eq!(versions.len(), 1);
        assert!(versions.contains(&&Version::from("v4")));
    }

    #[test]
    fn add_multiple_versions() {
        let mut set = ActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));
        set.add(&make_interpreted("actions/checkout", "v3", None));

        let versions: Vec<_> = set
            .versions_for(&ActionId::from("actions/checkout"))
            .collect();
        assert_eq!(versions.len(), 2);
        assert!(versions.contains(&&Version::from("v4")));
        assert!(versions.contains(&&Version::from("v3")));
    }

    #[test]
    fn add_duplicate_version() {
        let mut set = ActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));
        set.add(&make_interpreted("actions/checkout", "v4", None));

        assert_eq!(
            set.versions_for(&ActionId::from("actions/checkout"))
                .count(),
            1
        );
    }

    #[test]
    fn action_ids() {
        let mut set = ActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));
        set.add(&make_interpreted("actions/setup-node", "v3", None));

        let ids: Vec<_> = set.action_ids().collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&&ActionId::from("actions/checkout")));
        assert!(ids.contains(&&ActionId::from("actions/setup-node")));
    }

    #[test]
    fn versions_for_unknown_action() {
        let set = ActionSet::new();
        assert_eq!(
            set.versions_for(&ActionId::from("unknown/action")).count(),
            0
        );
    }
}

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

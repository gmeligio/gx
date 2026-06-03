//! The `on:` trigger model and its scalar/list/map deserializer.

use serde::de::{Deserializer, MapAccess, Visitor};
use std::fmt;

/// A GitHub Actions trigger event.
///
/// Multi-trigger workflows hold a `Vec<Trigger>` in `Parsed::on`. Unrecognized event
/// names round-trip as `Other(String)` so rule logic never silently drops triggers it
/// has not been taught about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    PullRequest,
    PullRequestTarget,
    Push,
    Schedule,
    WorkflowDispatch,
    WorkflowCall,
    WorkflowRun,
    Release,
    /// Sub-filter under `push:`; rarely a top-level event but included for symmetry.
    Tags,
    Other(String),
}

impl Trigger {
    /// Maps a raw event name to a `Trigger`, falling back to `Other` for unknown names.
    fn from_name(name: &str) -> Self {
        match name {
            "pull_request" => Self::PullRequest,
            "pull_request_target" => Self::PullRequestTarget,
            "push" => Self::Push,
            "schedule" => Self::Schedule,
            "workflow_dispatch" => Self::WorkflowDispatch,
            "workflow_call" => Self::WorkflowCall,
            "workflow_run" => Self::WorkflowRun,
            "release" => Self::Release,
            "tags" => Self::Tags,
            other => Self::Other(other.to_owned()),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::PullRequest => "pull_request",
            Self::PullRequestTarget => "pull_request_target",
            Self::Push => "push",
            Self::Schedule => "schedule",
            Self::WorkflowDispatch => "workflow_dispatch",
            Self::WorkflowCall => "workflow_call",
            Self::WorkflowRun => "workflow_run",
            Self::Release => "release",
            Self::Tags => "tags",
            Self::Other(s) => s.as_str(),
        }
    }
}

impl fmt::Display for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `on:` field's three shapes: a bare event name, a list of event names, or a map of
/// event names to filter objects. We only need the set of event names for rule logic.
fn parse_triggers<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<Trigger>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Vec<Trigger>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a YAML string, list, or map describing workflow triggers")
        }

        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Vec<Trigger>, E> {
            Ok(vec![Trigger::from_name(v)])
        }
        fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Vec<Trigger>, E> {
            Ok(vec![Trigger::from_name(&v)])
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Vec<Trigger>, A::Error> {
            let mut out = Vec::new();
            while let Some(name) = seq.next_element::<String>()? {
                out.push(Trigger::from_name(&name));
            }
            Ok(out)
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<Trigger>, A::Error> {
            let mut out = Vec::new();
            while let Some(name) = map.next_key::<String>()? {
                // Discard the filter body; rules only need the event-name set.
                let _: serde::de::IgnoredAny = map.next_value()?;
                out.push(Trigger::from_name(&name));
            }
            Ok(out)
        }
    }
    de.deserialize_any(V)
}

/// Deserializes the `on:` block, wrapping the parsed triggers in `Some`.
pub(super) fn parse_triggers_opt<'de, D: Deserializer<'de>>(
    de: D,
) -> Result<Option<Vec<Trigger>>, D::Error> {
    parse_triggers(de).map(Some)
}

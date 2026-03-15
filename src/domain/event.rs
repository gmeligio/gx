use super::action::identity::{ActionId, Version};
use super::action::spec::Spec;
use std::fmt;

/// Observable transitions produced by domain operations (manifest sync, lock sync, etc.).
///
/// Domain methods return `Vec<Event>` instead of accepting `on_progress` callbacks,
/// keeping them pure and presentation-free. Command orchestrators iterate events and
/// call `on_progress(&event.to_string())` for each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A new action was added to the manifest.
    ActionAdded(Spec),
    /// An action was removed from the manifest.
    ActionRemoved(ActionId),
    /// An action's version was corrected from a SHA to the tag it points to.
    VersionCorrected {
        id: ActionId,
        corrected: Version,
        sha_points_to: Version,
    },
    /// A SHA version in the manifest was upgraded to the best matching tag.
    ShaUpgraded { id: ActionId, tag: Version },
    /// A lock resolution was skipped due to a recoverable error.
    ResolutionSkipped { spec: Spec, reason: String },
    /// Multiple actions were skipped due to recoverable errors.
    RecoverableWarning { count: usize },
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::ActionAdded(spec) => write!(f, "+ {spec}"),
            Event::ActionRemoved(id) => write!(f, "- {id}"),
            Event::VersionCorrected {
                id,
                corrected,
                sha_points_to,
            } => write!(
                f,
                "Corrected {id} version to {corrected} (SHA {sha_points_to} points to {corrected})"
            ),
            Event::ShaUpgraded { id, tag } => write!(f, "~ {id} SHA upgraded to {tag}"),
            Event::ResolutionSkipped { spec, reason } => {
                write!(f, "Skipping {spec}: {reason}")
            }
            Event::RecoverableWarning { count } => write!(
                f,
                "{count} action(s) skipped due to recoverable errors — run `gx tidy` again to retry."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Event;
    use crate::domain::action::identity::{ActionId, Version};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;

    #[test]
    fn display_action_added() {
        let spec = Spec::new(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        let event = Event::ActionAdded(spec);
        assert_eq!(event.to_string(), "+ actions/checkout@^4");
    }

    #[test]
    fn display_action_removed() {
        let event = Event::ActionRemoved(ActionId::from("actions/old-action"));
        assert!(event.to_string().contains("actions/old-action"));
    }

    #[test]
    fn display_sha_upgraded() {
        let event = Event::ShaUpgraded {
            id: ActionId::from("actions/checkout"),
            tag: Version::from("v4.1.0"),
        };
        assert_eq!(
            event.to_string(),
            "~ actions/checkout SHA upgraded to v4.1.0"
        );
    }

    #[test]
    fn display_resolution_skipped() {
        let spec = Spec::new(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        let event = Event::ResolutionSkipped {
            spec,
            reason: "rate limited".to_owned(),
        };
        assert!(event.to_string().contains("Skipping"));
        assert!(event.to_string().contains("rate limited"));
    }

    #[test]
    fn display_recoverable_warning() {
        let event = Event::RecoverableWarning { count: 3 };
        assert!(event.to_string().contains("3 action(s) skipped"));
    }

    #[test]
    fn display_version_corrected() {
        let event = Event::VersionCorrected {
            id: ActionId::from("actions/checkout"),
            corrected: Version::from("v4"),
            sha_points_to: Version::from("v4"),
        };
        assert!(event.to_string().contains("Corrected"));
        assert!(event.to_string().contains("actions/checkout"));
    }
}

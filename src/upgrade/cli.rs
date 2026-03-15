use super::types::{Mode, Request, Scope};
use crate::domain::action::identity::{ActionId, Version};
use thiserror::Error;

/// Errors from resolving CLI arguments into an [`Request`].
#[derive(Debug, Error)]
pub enum Error {
    /// `--latest` was combined with an exact version pin (`ACTION@VERSION`).
    #[error(
        "--latest cannot be combined with an exact version pin (ACTION@VERSION). \
         Use --latest ACTION to upgrade to latest, or ACTION@VERSION to pin."
    )]
    LatestWithVersionPin,

    /// The action string could not be parsed as `ACTION@VERSION`.
    #[error("invalid format: expected ACTION@VERSION (e.g., actions/checkout@v5), got: {input}")]
    InvalidActionFormat { input: String },
}

/// Resolve CLI arguments into an [`Request`].
///
/// # Errors
///
/// Returns [`Error`] for invalid upgrade mode combinations.
pub fn resolve_upgrade_mode(action: Option<&str>, latest: bool) -> Result<Request, Error> {
    match (action, latest) {
        (None, true) => Ok(Request::new(Mode::Latest, Scope::All)),
        (Some(action_str), true) => {
            if action_str.contains('@') {
                return Err(Error::LatestWithVersionPin);
            }
            let id = ActionId::from(action_str);
            Ok(Request::new(Mode::Latest, Scope::Single(id)))
        }
        (Some(action_str), false) => {
            if action_str.contains('@') {
                // Parse manually to keep Version for the pinned tag (not Specifier)
                let (action_part, version_part) =
                    action_str
                        .rsplit_once('@')
                        .ok_or_else(|| Error::InvalidActionFormat {
                            input: action_str.to_owned(),
                        })?;
                let id = ActionId::from(action_part);
                let version = Version::from(version_part);
                Ok(Request::new(Mode::Safe, Scope::Pinned(id, version)))
            } else {
                let id = ActionId::from(action_str);
                Ok(Request::new(Mode::Safe, Scope::Single(id)))
            }
        }
        (None, false) => Ok(Request::new(Mode::Safe, Scope::All)),
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Error, Mode, Scope, resolve_upgrade_mode};

    #[test]
    fn resolve_none_false_returns_safe_all() {
        let req = resolve_upgrade_mode(None, false).unwrap();
        assert!(matches!(req.mode, Mode::Safe));
        assert!(matches!(req.scope, Scope::All));
    }

    #[test]
    fn resolve_none_true_returns_latest_all() {
        let req = resolve_upgrade_mode(None, true).unwrap();
        assert!(matches!(req.mode, Mode::Latest));
        assert!(matches!(req.scope, Scope::All));
    }

    #[test]
    fn resolve_action_without_at_false_returns_safe_single() {
        let req = resolve_upgrade_mode(Some("actions/checkout"), false).unwrap();
        assert!(matches!(req.mode, Mode::Safe));
        assert!(matches!(req.scope, Scope::Single(_)));
    }

    #[test]
    fn resolve_action_without_at_true_returns_latest_single() {
        let req = resolve_upgrade_mode(Some("actions/checkout"), true).unwrap();
        assert!(matches!(req.mode, Mode::Latest));
        assert!(matches!(req.scope, Scope::Single(_)));
    }

    #[test]
    fn resolve_action_with_version_returns_pinned() {
        let req = resolve_upgrade_mode(Some("actions/checkout@v5"), false).unwrap();
        assert!(matches!(req.mode, Mode::Safe));
        assert!(matches!(req.scope, Scope::Pinned(_, _)));
    }

    #[test]
    fn resolve_latest_with_version_pin_returns_error() {
        let err = resolve_upgrade_mode(Some("actions/checkout@v5"), true).unwrap_err();
        assert!(matches!(err, Error::LatestWithVersionPin));
    }
}

use super::types::{UpgradeMode, UpgradeRequest, UpgradeScope};
use crate::domain::{ActionId, Version};
use thiserror::Error;

/// Errors from resolving CLI arguments into an [`UpgradeRequest`].
#[derive(Debug, Error)]
pub enum ResolveError {
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

/// Resolve CLI arguments into an [`UpgradeRequest`].
///
/// # Errors
///
/// Returns [`ResolveError`] for invalid upgrade mode combinations.
///
/// # Panics
///
/// Panics if `UpgradeRequest::new` rejects a known-valid mode/scope combination.
pub fn resolve_upgrade_mode(
    action: Option<&str>,
    latest: bool,
) -> Result<UpgradeRequest, ResolveError> {
    match (action, latest) {
        (None, true) => Ok(UpgradeRequest::new(UpgradeMode::Latest, UpgradeScope::All)
            .expect("Latest + All is always valid")),
        (Some(action_str), true) => {
            if action_str.contains('@') {
                return Err(ResolveError::LatestWithVersionPin);
            }
            let id = ActionId::from(action_str);
            Ok(
                UpgradeRequest::new(UpgradeMode::Latest, UpgradeScope::Single(id))
                    .expect("Latest + Single is always valid"),
            )
        }
        (Some(action_str), false) => {
            if action_str.contains('@') {
                // Parse manually to keep Version for the pinned tag (not Specifier)
                let (action_part, version_part) = action_str.rsplit_once('@').ok_or_else(|| {
                    ResolveError::InvalidActionFormat {
                        input: action_str.to_string(),
                    }
                })?;
                let id = ActionId::from(action_part);
                let version = Version::from(version_part);
                Ok(
                    UpgradeRequest::new(UpgradeMode::Pinned(version), UpgradeScope::Single(id))
                        .expect("Pinned + Single is always valid"),
                )
            } else {
                let id = ActionId::from(action_str);
                Ok(
                    UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::Single(id))
                        .expect("Safe + Single is always valid"),
                )
            }
        }
        (None, false) => Ok(UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All)
            .expect("Safe + All is always valid")),
    }
}

#[cfg(test)]
mod tests {
    use super::{ResolveError, UpgradeMode, UpgradeScope, resolve_upgrade_mode};

    #[test]
    fn resolve_none_false_returns_safe_all() {
        let req = resolve_upgrade_mode(None, false).unwrap();
        assert!(matches!(req.mode, UpgradeMode::Safe));
        assert!(matches!(req.scope, UpgradeScope::All));
    }

    #[test]
    fn resolve_none_true_returns_latest_all() {
        let req = resolve_upgrade_mode(None, true).unwrap();
        assert!(matches!(req.mode, UpgradeMode::Latest));
        assert!(matches!(req.scope, UpgradeScope::All));
    }

    #[test]
    fn resolve_action_without_at_false_returns_safe_single() {
        let req = resolve_upgrade_mode(Some("actions/checkout"), false).unwrap();
        assert!(matches!(req.mode, UpgradeMode::Safe));
        assert!(matches!(req.scope, UpgradeScope::Single(_)));
    }

    #[test]
    fn resolve_action_without_at_true_returns_latest_single() {
        let req = resolve_upgrade_mode(Some("actions/checkout"), true).unwrap();
        assert!(matches!(req.mode, UpgradeMode::Latest));
        assert!(matches!(req.scope, UpgradeScope::Single(_)));
    }

    #[test]
    fn resolve_action_with_version_returns_pinned() {
        let req = resolve_upgrade_mode(Some("actions/checkout@v5"), false).unwrap();
        assert!(matches!(req.mode, UpgradeMode::Pinned(_)));
        assert!(matches!(req.scope, UpgradeScope::Single(_)));
    }

    #[test]
    fn resolve_latest_with_version_pin_returns_error() {
        let err = resolve_upgrade_mode(Some("actions/checkout@v5"), true).unwrap_err();
        assert!(matches!(err, ResolveError::LatestWithVersionPin));
    }
}

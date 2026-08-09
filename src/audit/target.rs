//! What audit checks see: one borrowed view per `gx.lock` row, and the `mutable-ref` check.
//!
//! Reading the lock rather than workflow files keeps one notion of "which actions exist" —
//! whatever the scanner found. [`targets`] is the only place that touches a lock row, so
//! checks are unaffected when its shape changes.

use super::check_name::CheckName;
use super::report::Finding;
use crate::config::Level;
use crate::domain::action::identity::{ActionId, CommitSha};
use crate::domain::action::uses_ref::RefType;
use crate::domain::lock::Lock;

/// One locked action, as an audit check sees it.
pub struct AuditTarget<'lock> {
    /// The action's identity, e.g. `actions/checkout`.
    pub id: &'lock ActionId,
    /// The version label recorded in the lock, e.g. `v4.2.1` or `main`.
    pub version: &'lock str,
    /// The commit the action is pinned to.
    pub sha: &'lock CommitSha,
    /// What kind of reference resolved to `sha`, when the lock recorded one.
    pub ref_type: Option<&'lock RefType>,
}

/// Project every lock entry into an [`AuditTarget`], in an order two runs can be diffed.
pub fn targets(lock: &Lock) -> Vec<AuditTarget<'_>> {
    let mut found: Vec<AuditTarget<'_>> = lock
        .entries()
        .map(|locked| AuditTarget {
            id: locked.id(),
            version: locked.version_label(),
            sha: locked.sha(),
            ref_type: locked.commit().ref_type.as_ref(),
        })
        .collect();
    found.sort_by_key(|target| (target.id.as_str(), target.version));
    found
}

/// gx writes branch pins itself, so a user on one likely meant it — say so, don't fail.
const MUTABLE_REF_SEVERITY: Level = Level::Warn;

/// Report every target pinned to a branch, whose SHA moves out from under the lock.
pub fn mutable_ref(target: &AuditTarget<'_>) -> Option<Finding> {
    if target.ref_type != Some(&RefType::Branch) {
        return None;
    }
    Some(Finding::new(
        CheckName::MutableRef,
        MUTABLE_REF_SEVERITY,
        format!(
            "{} is pinned to branch {}, which moves; {} is not a stable reference",
            target.id,
            target.version,
            target.sha.as_str()
        ),
    ))
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{AuditTarget, Level, mutable_ref, targets};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::lock::Lock;

    const SHA: &str = "abc123def456789012345678901234567890abcd";

    fn lock_with(action: &str, specifier: &str, label: &str, ref_type: RefType) -> Lock {
        let mut lock = Lock::default();
        let spec = Spec::new(ActionId::from(action), Specifier::parse(specifier));
        let reference = ResolvedRef::from_stored(Version::from(label), Some(&ref_type));
        lock.set(
            &spec,
            reference,
            Commit {
                sha: CommitSha::from(SHA),
                repository: Repository::from(action),
                ref_type: Some(ref_type),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
        lock
    }

    fn only_target(lock: &Lock) -> AuditTarget<'_> {
        let mut found = targets(lock);
        assert_eq!(found.len(), 1);
        found.remove(0)
    }

    #[test]
    fn branch_pin_is_reported_as_a_warning() {
        let lock = lock_with("actions/checkout", "main", "main", RefType::Branch);
        let finding = mutable_ref(&only_target(&lock)).expect("branch pin must be reported");

        assert_eq!(finding.level, Level::Warn);
        assert!(
            finding.message.contains("actions/checkout"),
            "finding must name the action: {}",
            finding.message
        );
        assert!(finding.message.contains("main"));
    }

    #[test]
    fn tag_pin_is_not_reported() {
        let lock = lock_with("actions/checkout", "^4", "v4.2.1", RefType::Tag);
        assert!(mutable_ref(&only_target(&lock)).is_none());
    }

    #[test]
    fn release_pin_is_not_reported() {
        let lock = lock_with("actions/checkout", "^4", "v4.2.1", RefType::Release);
        assert!(mutable_ref(&only_target(&lock)).is_none());
    }

    #[test]
    fn commit_pin_is_not_reported() {
        let lock = lock_with("actions/checkout", SHA, SHA, RefType::Commit);
        assert!(mutable_ref(&only_target(&lock)).is_none());
    }

    #[test]
    fn targets_projects_every_lock_entry() {
        let mut lock = lock_with("actions/checkout", "^4", "v4.2.1", RefType::Tag);
        let spec = Spec::new(
            ActionId::from("actions/setup-node"),
            Specifier::parse("main"),
        );
        lock.set(
            &spec,
            ResolvedRef::from_stored(Version::from("main"), Some(&RefType::Branch)),
            Commit {
                sha: CommitSha::from(SHA),
                repository: Repository::from("actions/setup-node"),
                ref_type: Some(RefType::Branch),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );

        let found = targets(&lock);

        assert_eq!(found.len(), 2);
        // Sorted by action id, so output ordering does not depend on map iteration order.
        assert_eq!(found[0].id.as_str(), "actions/checkout");
        assert_eq!(found[1].id.as_str(), "actions/setup-node");
        assert_eq!(found[1].version, "main");
    }

    #[test]
    fn empty_lock_yields_no_targets() {
        assert!(targets(&Lock::default()).is_empty());
    }
}

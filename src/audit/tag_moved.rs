//! The `tag-moved` check: has a pinned tag been repointed since gx locked it?
//!
//! Every major Actions supply-chain compromise has the same signature — an attacker
//! repoints existing tags at a malicious commit. The lock records where each tag pointed
//! when it was pinned, which is the only baseline that makes the movement detectable.

use super::check_name::CheckName;
use super::report::Finding;
use super::target::AuditTarget;
use crate::config::Level;
use crate::domain::action::identity::CommitSha;
use crate::domain::action::uses_ref::RefType;
use crate::infra::github::TagResolver;

/// How many leading SHA characters a message shows. Enough to identify a commit at a
/// glance without wrapping the line.
const SHORT_SHA: usize = 8;

/// A moved tag is either an upstream force-push or an attack; both must stop a build.
const TAG_MOVED_SEVERITY: Level = Level::Error;

/// "I could not look" must never render as "I looked and it is fine".
const TAG_UNVERIFIED_SEVERITY: Level = Level::Error;

/// The first `SHORT_SHA` characters, for a message a human reads.
fn short(sha: &str) -> &str {
    sha.get(..SHORT_SHA).unwrap_or(sha)
}

/// Whether this entry names a tag whose movement could be measured.
///
/// Reads `ref_type` off the target rather than going through [`crate::domain::action::resolved::ResolvedRef`],
/// which maps a missing kind to `Tag`. That guess is right for resolution, where being wrong
/// costs a failed lookup, and wrong here, where it costs a finding telling a user their
/// dependency may have been compromised. Reconcile the two by making the domain stricter.
fn is_eligible(target: &AuditTarget<'_>) -> bool {
    if !matches!(target.ref_type, Some(&RefType::Tag | &RefType::Release)) {
        return false;
    }
    // A row pairing ref_type "tag" with a SHA-shaped label — hand-edited, or legacy — would
    // otherwise produce a lookup for a tag named abc123…, a 404, and a finding accusing the
    // user of something that never happened.
    !CommitSha::is_valid(target.version)
}

/// Report whether `target`'s tag still points where the lock recorded.
pub fn tag_moved(target: &AuditTarget<'_>, resolver: &dyn TagResolver) -> Option<Finding> {
    if !is_eligible(target) {
        return None;
    }

    let current = match resolver.tag_commit(target.id, target.version) {
        Ok(sha) => sha,
        Err(error) => {
            return Some(Finding::new(
                CheckName::TagUnverified,
                TAG_UNVERIFIED_SEVERITY,
                format!(
                    "{} {} could not be verified: {error}",
                    target.id, target.version
                ),
            ));
        }
    };

    if current == *target.sha {
        return None;
    }

    Some(Finding::new(
        CheckName::TagMoved,
        TAG_MOVED_SEVERITY,
        format!(
            "{} {} now points to a different commit than when it was pinned (lock {}, now {}) — the tag was moved",
            target.id,
            target.version,
            short(target.sha.as_str()),
            short(current.as_str()),
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
    use super::{CheckName, Level, tag_moved};
    use crate::audit::target::{AuditTarget, targets};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::lock::Lock;
    use crate::infra::github::FakeTags;

    const PINNED: &str = "a284dc18b1d2b1a5c1e5e5f0e5e5e5e5e5e5e5e5";
    const MOVED: &str = "0e58ed86baaa5f0e5e5e5e5e5e5e5e5e5e5e5e5e";

    /// A lock holding one entry, built the way the resolver writes them.
    fn lock_with(action: &str, label: &str, sha: &str, ref_type: Option<RefType>) -> Lock {
        let mut lock = Lock::default();
        let spec = Spec::new(ActionId::from(action), Specifier::parse(label));
        lock.set(
            &spec,
            ResolvedRef::from_stored(Version::from(label), ref_type.as_ref()),
            Commit {
                sha: CommitSha::from(sha),
                repository: Repository::from(action),
                ref_type,
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
        lock
    }

    fn only(lock: &Lock) -> AuditTarget<'_> {
        let mut found = targets(lock);
        assert_eq!(found.len(), 1);
        found.remove(0)
    }

    #[test]
    fn unmoved_tag_produces_no_finding() {
        let lock = lock_with("actions/checkout", "v4.2.1", PINNED, Some(RefType::Tag));
        let fake = FakeTags::failing().with("actions/checkout", "v4.2.1", PINNED);

        assert!(tag_moved(&only(&lock), &fake).is_none());
        // The lookup happened — a "no finding" that skipped the check would be a false clean.
        assert_eq!(fake.lookups().len(), 1);
    }

    #[test]
    fn moved_tag_is_reported_with_both_shas() {
        let lock = lock_with("actions/checkout", "v4.2.1", PINNED, Some(RefType::Tag));
        let fake = FakeTags::failing().with("actions/checkout", "v4.2.1", MOVED);

        let finding = tag_moved(&only(&lock), &fake).expect("a moved tag must be reported");

        assert_eq!(finding.rule, CheckName::TagMoved);
        assert_eq!(finding.level, Level::Error);
        assert!(finding.message.contains("actions/checkout"));
        assert!(finding.message.contains("v4.2.1"));
        assert!(
            finding.message.contains(super::short(PINNED)),
            "{}",
            finding.message
        );
        assert!(
            finding.message.contains(super::short(MOVED)),
            "{}",
            finding.message
        );
        assert!(finding.message.contains("the tag was moved"));
    }

    #[test]
    fn unmoved_annotated_tag_produces_no_finding() {
        // The seam returns the commit an annotated tag targets, since dereferencing is the
        // adapter's contract. The check must trust that rather than re-deriving it.
        let lock = lock_with("actions/checkout", "v4.2.1", PINNED, Some(RefType::Tag));
        let fake = FakeTags::failing().with("actions/checkout", "v4.2.1", PINNED);

        assert!(tag_moved(&only(&lock), &fake).is_none());
    }

    #[test]
    fn release_entry_is_checked_like_a_tag() {
        let lock = lock_with("actions/checkout", "v4.2.1", PINNED, Some(RefType::Release));
        let fake = FakeTags::failing().with("actions/checkout", "v4.2.1", MOVED);

        let finding = tag_moved(&only(&lock), &fake).expect("a release entry must be checked");
        assert_eq!(finding.rule, CheckName::TagMoved);
    }

    #[test]
    fn subpath_action_resolves_against_its_repository_root() {
        let action = "github/codeql-action/upload-sarif";
        let lock = lock_with(action, "v3.28.0", PINNED, Some(RefType::Tag));
        // Seeded under the repo root: a lookup for the full path would miss and 404.
        let fake = FakeTags::failing().with("github/codeql-action", "v3.28.0", PINNED);

        assert!(
            tag_moved(&only(&lock), &fake).is_none(),
            "a healthy subpath action must not be reported"
        );
    }

    /// Entries that name no measurable tag: skipped, and never looked up.
    #[test]
    fn ineligible_entries_are_skipped_without_a_lookup() {
        let cases = [
            (
                "main",
                "abc123def456789012345678901234567890abcd",
                Some(RefType::Branch),
            ),
            (PINNED, PINNED, Some(RefType::Commit)),
            ("v4.2.1", PINNED, None),
        ];

        for (label, sha, ref_type) in cases {
            let kind = format!("{ref_type:?}");
            let lock = lock_with("actions/checkout", label, sha, ref_type);
            let fake = FakeTags::failing().with("actions/checkout", label, MOVED);

            assert!(
                tag_moved(&only(&lock), &fake).is_none(),
                "{kind} must not be reported"
            );
            // Without this, a skip that happened for the wrong reason would still pass.
            assert!(
                fake.lookups().is_empty(),
                "{kind} must not be looked up, got {:?}",
                fake.lookups()
            );
        }
    }

    #[test]
    fn sha_shaped_label_on_a_tag_row_is_skipped_without_a_lookup() {
        // Hand-edited or legacy: ref_type says tag, the label is a SHA. Looking that up
        // would 404 and accuse the user of tampering that never happened.
        let lock = lock_with("actions/checkout", PINNED, PINNED, Some(RefType::Tag));
        let fake = FakeTags::failing();

        assert!(tag_moved(&only(&lock), &fake).is_none());
        assert!(fake.lookups().is_empty(), "no tag named by a SHA exists");
    }

    #[test]
    fn failed_lookup_is_reported_under_its_own_name() {
        let lock = lock_with("actions/checkout", "v4.2.1", PINNED, Some(RefType::Tag));
        let fake = FakeTags::failing();

        let finding = tag_moved(&only(&lock), &fake).expect("a failed lookup must be reported");

        assert_eq!(finding.rule, CheckName::TagUnverified);
        assert_eq!(finding.level, Level::Error);
        assert!(finding.message.contains("could not be verified"));
        assert!(finding.message.contains("actions/checkout"));
    }

    #[test]
    fn one_failure_does_not_suppress_another_entrys_finding() {
        let mut lock = lock_with("actions/checkout", "v4.2.1", PINNED, Some(RefType::Tag));
        let spec = Spec::new(ActionId::from("actions/setup-node"), Specifier::parse("v4"));
        lock.set(
            &spec,
            ResolvedRef::from_stored(Version::from("v4"), Some(&RefType::Tag)),
            Commit {
                sha: CommitSha::from(PINNED),
                repository: Repository::from("actions/setup-node"),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
        // checkout's lookup fails; setup-node's tag has moved.
        let fake = FakeTags::failing().with("actions/setup-node", "v4", MOVED);

        let found: Vec<_> = targets(&lock)
            .iter()
            .filter_map(|target| tag_moved(target, &fake))
            .collect();

        assert_eq!(found.len(), 2, "both entries must report");
        assert_eq!(found[0].rule, CheckName::TagUnverified);
        assert_eq!(found[1].rule, CheckName::TagMoved);
    }
}

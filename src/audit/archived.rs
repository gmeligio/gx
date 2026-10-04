//! The `archived-action` check: is this action published from a repository that can no
//! longer publish?
//!
//! An archived repository accepts no commits, so a vulnerability found in it will never
//! get a patched version. Upgrading is not a remedy; migrating off it is the only one —
//! which is why this check reports before an advisory lands rather than after.

use super::check_name::CheckName;
use super::report::Finding;
use super::target::AuditTarget;
use crate::config::Level;
use crate::infra::github::RepoMetadata;

/// A frozen dependency is a risk about the future, not an exploit in the present, so this
/// does not fail a build.
const ARCHIVED_SEVERITY: Level = Level::Warn;

/// A lookup that did not happen must fail the run, or a green CI result would certify
/// nothing. `warn` would leave the exit code at 0.
const LOOKUP_FAILURE_SEVERITY: Level = Level::Error;

/// Report a target whose action comes from an archived repository.
///
/// `None` means one thing only: the lookup succeeded and the repository is active. Every
/// other outcome is a finding, so "could not check" can never read as "checked and clean".
pub fn archived_action(target: &AuditTarget<'_>, source: &dyn RepoMetadata) -> Option<Finding> {
    // Repo-level endpoint, so `github/codeql-action/init` must be asked about as
    // `github/codeql-action` — the subpath would 404.
    let repo = target.id.base_repo();

    let meta = match source.metadata(repo.as_str()) {
        Ok(meta) => meta,
        Err(reason) => {
            return Some(Finding::new(
                CheckName::ArchivedAction,
                LOOKUP_FAILURE_SEVERITY,
                // Leads with the uncertainty so a skimmed report cannot read this as a
                // verdict about the action. The reason is the error's own wording, which
                // is what distinguishes a rename from a rate-limit blip.
                format!(
                    "could not determine whether {} is archived: {reason}",
                    target.id
                ),
            ));
        }
    };

    if !meta.archived {
        return None;
    }

    Some(Finding::new(
        CheckName::ArchivedAction,
        ARCHIVED_SEVERITY,
        format!(
            "{} is published from an archived repository (last pushed {}); \
             it will receive no security fixes, so migrate to a maintained alternative",
            // The action as the user wrote it, subpath included, so they can find it in
            // their workflow — even though the lookup used the base repository.
            target.id,
            day_of(&meta.pushed_at),
        ),
    ))
}

/// The date half of an ISO-8601 timestamp; the time of day is noise at the granularity of
/// "how many years frozen".
///
/// Falls back to the whole string when there is no `T`. Nothing validates the shape
/// upstream, and blanking the date would turn a real archived-repository finding into a
/// misleading one.
fn day_of(timestamp: &str) -> &str {
    timestamp.split_once('T').map_or(timestamp, |(day, _)| day)
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{AuditTarget, Level, archived_action, day_of};
    use crate::domain::action::identity::{ActionId, CommitSha};
    use crate::infra::github::CannedRepos;

    const SHA: &str = "abc123def456789012345678901234567890abcd";

    /// A target built directly rather than through a lock, so these tests exercise the
    /// check alone and stay unaffected by how the lock projection changes.
    fn target<'lock>(id: &'lock ActionId, sha: &'lock CommitSha) -> AuditTarget<'lock> {
        AuditTarget {
            id,
            version: "v1",
            sha,
            ref_type: None,
        }
    }

    #[test]
    fn archived_repository_is_reported_as_a_warning() {
        let id = ActionId::from("actions/setup-ruby");
        let sha = CommitSha::from(SHA);
        let source = CannedRepos::archived("2021-04-14T18:22:31Z");

        let finding =
            archived_action(&target(&id, &sha), &source).expect("archived repo must be reported");

        assert_eq!(finding.level, Level::Warn);
        assert!(
            finding.message.contains("actions/setup-ruby"),
            "must name the action: {}",
            finding.message
        );
        assert!(
            finding.message.contains("2021-04-14"),
            "must state how stale the dependency is: {}",
            finding.message
        );
        assert!(
            finding.message.contains("migrate"),
            "must point at migration, not upgrade: {}",
            finding.message
        );
        assert!(
            !finding.message.contains("18:22:31"),
            "time of day is noise at this granularity: {}",
            finding.message
        );
    }

    #[test]
    fn active_repository_is_not_reported_but_is_still_looked_up() {
        let id = ActionId::from("actions/checkout");
        let sha = CommitSha::from(SHA);
        let source = CannedRepos::active();

        assert!(archived_action(&target(&id, &sha), &source).is_none());
        // Asserting the lookup happened is what stops this passing vacuously: silence
        // because the repo is active, not because the check never ran.
        assert_eq!(source.seen.borrow().as_slice(), ["actions/checkout"]);
    }

    #[test]
    fn subpath_action_is_looked_up_by_its_base_repository() {
        let id = ActionId::from("github/codeql-action/upload-sarif");
        let sha = CommitSha::from(SHA);
        let source = CannedRepos::active();

        archived_action(&target(&id, &sha), &source);

        // The subpath has no repository of its own, so asking for it would 404 — turning
        // a healthy workflow into an error finding.
        assert_eq!(source.seen.borrow().as_slice(), ["github/codeql-action"]);
    }

    #[test]
    fn subpath_action_is_reported_under_its_full_name() {
        let id = ActionId::from("github/codeql-action/upload-sarif");
        let sha = CommitSha::from(SHA);
        let source = CannedRepos::archived("2021-04-14T18:22:31Z");

        let finding =
            archived_action(&target(&id, &sha), &source).expect("archived repo must be reported");

        // Looked up by base repo, but reported by the name the user wrote, or they cannot
        // find it in their workflow.
        assert!(
            finding
                .message
                .contains("github/codeql-action/upload-sarif"),
            "must name the action including its subpath: {}",
            finding.message
        );
    }

    #[test]
    fn failed_lookup_is_an_error_naming_the_action_and_the_reason() {
        let id = ActionId::from("actions/checkout");
        let sha = CommitSha::from(SHA);
        let source = CannedRepos::failing();

        let finding = archived_action(&target(&id, &sha), &source)
            .expect("a failed lookup must not read as clean");

        // Error, not warn: warn leaves the exit code at 0, so CI would stay green having
        // checked nothing.
        assert_eq!(finding.level, Level::Error);
        assert!(
            finding.message.contains("could not determine"),
            "must lead with the uncertainty, not a verdict: {}",
            finding.message
        );
        assert!(
            finding.message.contains("actions/checkout"),
            "must name the action: {}",
            finding.message
        );
        assert!(
            finding.message.contains("not found"),
            "must carry the underlying reason: {}",
            finding.message
        );
    }

    #[test]
    fn a_timestamp_without_a_time_is_shown_whole() {
        // Nothing validates the shape upstream. Blanking the date would make a real
        // archived-repository finding read as if gx had no idea how stale it was.
        assert_eq!(day_of("2021-04-14"), "2021-04-14");
        assert_eq!(day_of("2021-04-14T18:22:31Z"), "2021-04-14");
        assert_eq!(day_of(""), "");
    }
}

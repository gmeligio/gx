use crate::domain::action::identity::CommitSha;
use crate::domain::action::specifier::Specifier;
use crate::domain::action::uses_ref::RefType;

/// Metadata about a resolved action entry in the lock file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The resolved commit SHA
    pub sha: CommitSha,
    /// The most specific version tag pointing to this SHA
    pub version: Option<String>,
    /// Human-readable version comment for workflow output (e.g. "v6")
    pub comment: String,
    /// The GitHub repository that was queried
    pub repository: String,
    /// The type of reference that was resolved
    pub ref_type: Option<RefType>,
    /// RFC 3339 timestamp of the commit (meaning depends on `ref_type`)
    pub date: String,
}

impl Entry {
    /// Create a new lock entry.
    #[must_use]
    pub fn new(
        sha: CommitSha,
        repository: String,
        ref_type: Option<RefType>,
        date: String,
    ) -> Self {
        Self {
            sha,
            version: None,
            comment: String::new(),
            repository,
            ref_type,
            date,
        }
    }

    /// Create a lock entry with version and comment.
    #[must_use]
    pub fn with_version_and_comment(
        sha: CommitSha,
        version: Option<String>,
        comment: String,
        repository: String,
        ref_type: Option<RefType>,
        date: String,
    ) -> Self {
        Self {
            sha,
            version,
            comment,
            repository,
            ref_type,
            date,
        }
    }

    /// Check if this lock entry is complete for the given specifier.
    ///
    /// An entry is complete when:
    /// - version is present and non-empty
    /// - comment is non-empty (or specifier is non-semver)
    /// - repository is non-empty
    /// - date is non-empty
    /// - `ref_type` is set
    #[must_use]
    pub fn is_complete(&self, specifier: &Specifier) -> bool {
        let Self {
            sha: _, // CommitSha is always valid by construction
            version,
            comment,
            repository,
            ref_type,
            date,
        } = self;

        let version_ok = version.as_ref().is_some_and(|v| !v.is_empty());
        let repository_ok = !repository.is_empty();
        let date_ok = !date.is_empty();
        let ref_type_ok = ref_type.is_some();

        // For semver ranges, comment must match the specifier's expected comment; for Ref/Sha specifiers it's optional
        let comment_ok = match specifier {
            Specifier::Range { .. } => comment == specifier.to_comment(),
            _ => true,
        };

        version_ok && repository_ok && date_ok && ref_type_ok && comment_ok
    }

    /// Set the version field on this entry.
    pub fn set_version(&mut self, version: Option<String>) {
        self.version = version;
    }

    /// Set the comment field on this entry.
    pub fn set_comment(&mut self, comment: String) {
        self.comment = comment;
    }
}

#[cfg(test)]
mod tests {
    use super::Entry;
    use crate::domain::action::identity::CommitSha;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;

    #[test]
    fn test_is_complete_with_all_fields() {
        let entry = Entry::with_version_and_comment(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4.0.0".to_string()),
            "v4".to_string(),
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(entry.is_complete(&Specifier::parse("^4")));
    }

    #[test]
    fn test_is_complete_missing_ref_type() {
        let entry = Entry::with_version_and_comment(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4.0.0".to_string()),
            "v4".to_string(),
            "actions/checkout".to_string(),
            None,
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(!entry.is_complete(&Specifier::parse("^4")));
    }

    #[test]
    fn test_is_complete_missing_comment_for_range() {
        let entry = Entry::with_version_and_comment(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4.0.0".to_string()),
            String::new(),
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(!entry.is_complete(&Specifier::parse("^4")));
    }

    #[test]
    fn test_is_complete_missing_version() {
        let entry = Entry::with_version_and_comment(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            None,
            "v4".to_string(),
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(!entry.is_complete(&Specifier::parse("^4")));
    }

    #[test]
    fn test_is_complete_missing_date() {
        let entry = Entry::with_version_and_comment(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4.0.0".to_string()),
            "v4".to_string(),
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            String::new(),
        );
        assert!(!entry.is_complete(&Specifier::parse("^4")));
    }

    #[test]
    fn test_is_complete_non_semver_ref() {
        // Branch refs have empty comment, that's OK
        let entry = Entry::with_version_and_comment(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("main".to_string()),
            String::new(),
            "actions/checkout".to_string(),
            Some(RefType::Branch),
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(entry.is_complete(&Specifier::parse("main")));
    }

    #[test]
    fn test_is_complete_patch_version() {
        let entry = Entry::with_version_and_comment(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4.1.0".to_string()),
            "v4.1.0".to_string(),
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(entry.is_complete(&Specifier::parse("~4.1.0")));
    }

    #[test]
    fn test_is_complete_minor_version() {
        let entry = Entry::with_version_and_comment(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4.1.0".to_string()),
            "v4.1".to_string(),
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(entry.is_complete(&Specifier::parse("^4.1")));
    }

    // --- Lock completeness lifecycle tests (migrated from tidy/tests.rs) ---

    #[test]
    fn test_lock_completeness_missing_specifier_derived() {
        // Entry with version but empty comment — not complete for a Range specifier
        let mut entry = Entry::with_version_and_comment(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4".to_string()),
            String::new(), // empty comment = not complete for Range
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        );
        let specifier = Specifier::from_v1("v4");
        assert!(!entry.is_complete(&specifier));

        // Fix: derive and populate the comment
        let comment = specifier.to_comment().to_string();
        entry.set_comment(comment);

        assert!(entry.is_complete(&specifier));
        assert_eq!(entry.comment, "v4");
    }

    #[test]
    fn test_lock_completeness_missing_version_refined() {
        // Entry with comment but no version — not complete
        let mut entry = Entry::with_version_and_comment(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            None, // missing version
            "v4".to_string(),
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        );
        let specifier = Specifier::from_v1("v4");
        assert!(!entry.is_complete(&specifier));

        // Fix: populate version via REFINE
        entry.set_version(Some("v4".to_string()));

        assert!(entry.is_complete(&specifier));
        assert_eq!(entry.version, Some("v4".to_string()));
    }

    #[test]
    fn test_lock_completeness_complete_entry_unchanged() {
        // A fully-populated entry is already complete — no mutations needed
        let entry = Entry::with_version_and_comment(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4".to_string()),
            "v4".to_string(),
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(entry.is_complete(&Specifier::from_v1("v4")));
    }

    #[test]
    fn test_lock_completeness_manifest_version_precision_mismatch() {
        // Entry whose comment was set for the old specifier (v6) but specifier is now v6.1
        let mut entry = Entry::with_version_and_comment(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v6.0.2".to_string()),
            "v6".to_string(), // was correct for v6, wrong for v6.1
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        );
        let specifier = Specifier::from_v1("v6.1");
        assert!(!entry.is_complete(&specifier));

        // Fix: update comment to match new specifier
        let comment = specifier.to_comment().to_string();
        entry.set_comment(comment);

        assert!(entry.is_complete(&specifier));
        assert_eq!(entry.comment, "v6.1");
    }
}

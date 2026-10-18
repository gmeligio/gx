//! Conservative advisory range matching, distinct from dependency selection.

use semver::{Op, Version, VersionReq};

/// Whether an advisory's range covers a locked version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Match {
    /// The locked version falls inside the affected range.
    Affected,
    /// The locked version provably falls outside it.
    Unaffected,
    /// Neither could be established. Never collapsed into `Unaffected`.
    Undetermined,
}

/// How precisely a locked version names a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Precision {
    /// `41` — the whole 41 line.
    Major,
    /// `4.2` — the whole 4.2 line.
    Minor,
    /// `4.2.1` — one release.
    Exact,
}

/// A locked version parsed for comparison, remembering how precise it was.
struct Locked {
    /// The version zero-padded to three components, so `semver` can compare it.
    version: Version,
    /// What the original string actually named.
    precision: Precision,
}

/// Strip a conventional `v` prefix and zero-pad partial versions.
/// Branches, SHAs, and other unparseable labels remain unknown.
fn parse_locked(label: &str) -> Option<Locked> {
    let bare = label
        .strip_prefix('v')
        .or_else(|| label.strip_prefix('V'))
        .unwrap_or(label);

    // Split prereleases before counting components: `3.0.0-beta.2` is exact.
    let (core, suffix) = bare.split_once('-').map_or((bare, ""), |(c, s)| (c, s));
    let numeric = |part: &&str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
    let parts: Vec<&str> = core.split('.').collect();
    if !parts.iter().all(numeric) {
        return None;
    }
    let (padded, precision) = match parts.as_slice() {
        [major] => (format!("{major}.0.0"), Precision::Major),
        [major, minor] => (format!("{major}.{minor}.0"), Precision::Minor),
        [major, minor, patch] => (format!("{major}.{minor}.{patch}"), Precision::Exact),
        _ => return None,
    };
    let full = if suffix.is_empty() {
        padded
    } else {
        format!("{padded}-{suffix}")
    };
    Version::parse(&full)
        .ok()
        .map(|version| Locked { version, precision })
}

/// Whether a partial pin can overlap a range even though its zero-padded floor does not.
/// Equality, caret, and tilde constraints have lower bounds too. Every comparator must
/// allow overlap; a definitely disjoint comparator still proves the conjunction safe.
/// Upper bounds deliberately use the floor, conservatively favoring a finding.
fn partial_overlap(locked: &Locked, range: &VersionReq) -> bool {
    if locked.precision == Precision::Exact {
        return false;
    }
    range.comparators.iter().all(|bound| {
        // `>2` excludes all 2.x releases; `>2.5` excludes all 2.5.x releases.
        let excludes_line = bound.op == Op::Greater
            && match locked.precision {
                Precision::Major => bound.minor.is_none(),
                Precision::Minor => bound.patch.is_none(),
                Precision::Exact => false,
            };
        bound.matches(&locked.version)
            || (!excludes_line
                && !matches!(bound.op, Op::Less | Op::LessEq)
                && bound.major == locked.version.major
                && (locked.precision == Precision::Major
                    || bound.minor.is_none()
                    || bound.minor == Some(locked.version.minor)))
    })
}

/// Decide whether an advisory's affected range covers a locked version.
pub(super) fn matches(label: &str, range: &str) -> Match {
    let (Some(locked), Ok(requirement)) = (parse_locked(label), VersionReq::parse(range)) else {
        return Match::Undetermined;
    };
    if requirement.matches(&locked.version) {
        return Match::Affected;
    }
    // Dependency requirements exclude prereleases unless explicitly opted into. That
    // exclusion is not evidence of safety for an advisory; retain positive matches but
    // warn on every negative prerelease result rather than inventing range semantics.
    if !locked.version.pre.is_empty() || partial_overlap(&locked, &requirement) {
        return Match::Undetermined;
    }
    Match::Unaffected
}

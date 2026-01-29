use semver::Version;

/// Attempts to parse a version string into a semver Version.
/// Handles common formats like "v4", "v4.1", "v4.1.2", "4.1.2"
fn parse_semver(version: &str) -> Option<Version> {
    // Strip leading 'v' or 'V' if present
    let normalized = version
        .strip_prefix('v')
        .or_else(|| version.strip_prefix('V'))
        .unwrap_or(version);

    // Try parsing as-is
    if let Ok(v) = Version::parse(normalized) {
        return Some(v);
    }

    // Try adding .0 for versions like "4.1"
    let with_patch = format!("{}.0", normalized);
    if let Ok(v) = Version::parse(&with_patch) {
        return Some(v);
    }

    // Try adding .0.0 for versions like "4"
    let with_minor_patch = format!("{}.0.0", normalized);
    if let Ok(v) = Version::parse(&with_minor_patch) {
        return Some(v);
    }

    None
}

/// Compares two version strings and returns the higher one.
/// If both are valid semver, uses semver comparison.
/// If only one is valid semver, that one wins.
/// If neither is valid semver, returns the first one.
fn higher_version<'a>(a: &'a str, b: &'a str) -> &'a str {
    let parsed_a = parse_semver(a);
    let parsed_b = parse_semver(b);

    match (parsed_a, parsed_b) {
        (Some(va), Some(vb)) => {
            if va >= vb {
                a
            } else {
                b
            }
        }
        (Some(_), None) => a,
        (None, Some(_)) => b,
        (None, None) => a, // Default to first if neither is semver
    }
}

/// Finds the highest semver version from a list of versions.
/// Returns None if the list is empty.
pub fn find_highest_version<'a>(versions: &[&'a str]) -> Option<&'a str> {
    versions.iter().copied().reduce(higher_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_semver_full() {
        let v = parse_semver("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_parse_semver_with_v_prefix() {
        let v = parse_semver("v1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_parse_semver_major_only() {
        let v = parse_semver("v4").unwrap();
        assert_eq!(v.major, 4);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_parse_semver_major_minor() {
        let v = parse_semver("v4.1").unwrap();
        assert_eq!(v.major, 4);
        assert_eq!(v.minor, 1);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_parse_semver_branch_returns_none() {
        assert!(parse_semver("main").is_none());
        assert!(parse_semver("develop").is_none());
    }

    #[test]
    fn test_parse_semver_sha_returns_none() {
        assert!(parse_semver("a1b2c3d4e5f6").is_none());
    }

    #[test]
    fn test_higher_version_both_semver() {
        assert_eq!(higher_version("v4", "v3"), "v4");
        assert_eq!(higher_version("v3", "v4"), "v4");
        assert_eq!(higher_version("v4.1", "v4.0"), "v4.1");
        assert_eq!(higher_version("v4.0.1", "v4.0.0"), "v4.0.1");
    }

    #[test]
    fn test_higher_version_one_semver() {
        assert_eq!(higher_version("v4", "main"), "v4");
        assert_eq!(higher_version("main", "v4"), "v4");
    }

    #[test]
    fn test_higher_version_neither_semver() {
        assert_eq!(higher_version("main", "develop"), "main");
    }

    #[test]
    fn test_find_highest_version() {
        let versions = vec!["v3", "v4", "v2"];
        assert_eq!(find_highest_version(&versions), Some("v4"));
    }

    #[test]
    fn test_find_highest_version_empty() {
        let versions: Vec<&str> = vec![];
        assert_eq!(find_highest_version(&versions), None);
    }
}

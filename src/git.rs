use serde::Deserialize;

/// Check if a string is a full commit SHA (40 hexadecimal characters)
pub fn is_commit_sha(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Git ref structure returned by the GitHub API
#[derive(Debug, Deserialize)]
pub struct GitRef {
    pub object: GitObject,
}

/// Git object containing a SHA
#[derive(Debug, Deserialize)]
pub struct GitObject {
    pub sha: String,
}

/// Structure for git ref entries returned by the refs API
#[derive(Debug, Deserialize)]
pub struct GitRefEntry {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub object: GitObject,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_commit_sha_valid() {
        assert!(is_commit_sha("a1b2c3d4e5f6789012345678901234567890abcd"));
        assert!(is_commit_sha("0000000000000000000000000000000000000000"));
        assert!(is_commit_sha("ffffffffffffffffffffffffffffffffffffffff"));
    }

    #[test]
    fn test_is_commit_sha_invalid_length() {
        assert!(!is_commit_sha("abc123")); // Too short
        assert!(!is_commit_sha("a1b2c3d4e5f6789012345678901234567890abcde")); // Too long (41 chars)
        assert!(!is_commit_sha("")); // Empty
    }

    #[test]
    fn test_is_commit_sha_invalid_chars() {
        assert!(!is_commit_sha("g1b2c3d4e5f6789012345678901234567890abcd")); // 'g' is not hex
        assert!(!is_commit_sha("a1b2c3d4e5f6789012345678901234567890abc!")); // '!' is not hex
    }
}

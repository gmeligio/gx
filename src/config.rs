use anyhow::{Context, Result};
use std::env;

/// Application configuration loaded from environment variables
#[derive(Debug, Clone)]
pub struct Config {
    /// GitHub API token for authenticated requests
    pub github_token: Option<String>,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            github_token: env::var("GITHUB_TOKEN").ok(),
        }
    }

    /// Get GitHub token or return an error if not set
    pub fn require_github_token(&self) -> Result<&str> {
        self.github_token.as_deref().context(
            "GITHUB_TOKEN environment variable is required for this operation.\n\
                     Please set it with: export GITHUB_TOKEN=<your-token>\n\
                     You can create a token at: https://github.com/settings/tokens",
        )
    }

    /// Check if GitHub token is available
    pub fn has_github_token(&self) -> bool {
        self.github_token.is_some()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_without_token() {
        // Clear environment variable for test
        unsafe { env::remove_var("GITHUB_TOKEN") };

        let config = Config::from_env();
        assert!(!config.has_github_token());
        assert!(config.require_github_token().is_err());
    }

    #[test]
    fn test_config_with_token() {
        // Set environment variable for test
        unsafe { env::set_var("GITHUB_TOKEN", "test_token_123") };

        let config = Config::from_env();
        assert!(config.has_github_token());
        assert_eq!(config.require_github_token().unwrap(), "test_token_123");

        // Clean up
        unsafe { env::remove_var("GITHUB_TOKEN") };
    }

    #[test]
    fn test_require_github_token_error_message() {
        unsafe { env::remove_var("GITHUB_TOKEN") };

        let config = Config::from_env();
        let err = config.require_github_token().unwrap_err();
        let err_msg = format!("{:#}", err);

        assert!(err_msg.contains("GITHUB_TOKEN"));
        assert!(err_msg.contains("export GITHUB_TOKEN"));
    }
}

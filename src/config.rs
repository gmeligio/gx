use std::env;

/// Application configuration loaded from environment variables
#[derive(Debug, Clone)]
pub struct Config {
    /// GitHub API token for authenticated requests
    pub github_token: Option<String>,
    /// Whether to show verbose output
    pub verbose: bool,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            github_token: env::var("GITHUB_TOKEN").ok(),
            verbose: false,
        }
    }

    /// Set verbose mode
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
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
        unsafe { env::remove_var("GITHUB_TOKEN") };

        let config = Config::from_env();
        assert!(config.github_token.is_none());
    }

    #[test]
    fn test_config_with_token() {
        unsafe { env::set_var("GITHUB_TOKEN", "test_token_123") };

        let config = Config::from_env();
        assert_eq!(config.github_token, Some("test_token_123".to_string()));

        unsafe { env::remove_var("GITHUB_TOKEN") };
    }
}

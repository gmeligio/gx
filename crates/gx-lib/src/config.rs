use std::env;
use std::path::{Path, PathBuf};

use crate::commands::app::AppError;
use crate::domain::{Lock, Manifest};
use crate::infrastructure::{LOCK_FILE_NAME, MANIFEST_FILE_NAME, parse_lock, parse_manifest};

/// Runtime settings loaded from environment variables.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Github API token for authenticated requests
    pub github_token: Option<String>,
}

/// All application configuration, loaded once at startup.
#[derive(Debug)]
pub struct Config {
    pub settings: Settings,
    pub manifest: Manifest,
    pub lock: Lock,
    pub manifest_path: PathBuf,
    pub lock_path: PathBuf,
}

impl Settings {
    /// Load settings from environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            github_token: env::var("GITHUB_TOKEN").ok(),
        }
    }
}

impl Config {
    /// Load all configuration: settings from env, manifest and lock from disk.
    /// Paths are derived from `repo_root/.github/`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Manifest`] if the manifest file cannot be parsed.
    /// Returns [`AppError::Lock`] if the lock file cannot be parsed.
    pub fn load(repo_root: &Path) -> Result<Self, AppError> {
        let manifest_path = repo_root.join(".github").join(MANIFEST_FILE_NAME);
        let lock_path = repo_root.join(".github").join(LOCK_FILE_NAME);
        Ok(Self {
            settings: Settings::from_env(),
            manifest: parse_manifest(&manifest_path)?,
            lock: parse_lock(&lock_path)?,
            manifest_path,
            lock_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_has_no_token() {
        let settings = Settings::default();
        assert!(settings.github_token.is_none());
    }

    #[test]
    fn app_config_can_be_constructed_directly() {
        let config = Config {
            settings: Settings {
                github_token: Some("test_token".into()),
            },
            manifest: Manifest::default(),
            lock: Lock::default(),
            manifest_path: PathBuf::from("gx.toml"),
            lock_path: PathBuf::from("gx.lock"),
        };
        assert_eq!(config.settings.github_token, Some("test_token".to_string()));
    }

    #[test]
    fn app_config_load_returns_defaults_for_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        // No .github folder created — both files are missing
        let config = Config::load(dir.path()).unwrap();
        assert!(config.settings.github_token.is_none() || config.settings.github_token.is_some());
        assert!(config.manifest.specs().is_empty());
        assert!(config.lock.is_empty());
        assert!(config.manifest_path.ends_with("gx.toml"));
        assert!(config.lock_path.ends_with("gx.lock"));
    }
}

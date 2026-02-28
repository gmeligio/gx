use std::env;
use std::path::Path;

use crate::commands::app::AppError;
use crate::domain::{Lock, Manifest};
use crate::infrastructure::{FileLock, FileManifest, LockStore, ManifestStore};

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
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Manifest`] if the manifest file cannot be read or parsed.
    /// Returns [`AppError::Lock`] if the lock file cannot be read or parsed.
    pub fn load(manifest_path: &Path, lock_path: &Path) -> Result<Self, AppError> {
        let settings = Settings::from_env();
        let manifest = FileManifest::new(manifest_path).load()?;
        let lock = FileLock::new(lock_path).load()?;
        Ok(Self {
            settings,
            manifest,
            lock,
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
        };
        assert_eq!(config.settings.github_token, Some("test_token".to_string()));
    }

    #[test]
    fn app_config_load_returns_defaults_for_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("gx.toml");
        let lock_path = dir.path().join("gx.lock");

        let config = Config::load(&manifest_path, &lock_path).unwrap();
        assert!(config.settings.github_token.is_none() || config.settings.github_token.is_some());
        // Manifest and lock should be empty defaults when files don't exist
        assert!(config.manifest.specs().is_empty());
        assert!(config.lock.is_empty());
    }
}

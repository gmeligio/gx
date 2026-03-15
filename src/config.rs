use crate::domain::lock::Lock;
use crate::domain::manifest::Manifest;
use crate::infra::lock::{Error as LockFileError, LOCK_FILE_NAME, Store as LockStore};
use crate::infra::manifest::{Error as ManifestError, MANIFEST_FILE_NAME, parse_lint_config};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur when loading configuration.
#[derive(Debug, Error)]
pub enum Error {
    /// The manifest file cannot be parsed.
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    /// The lock file cannot be parsed.
    #[error(transparent)]
    Lock(#[from] LockFileError), // LockFileError is now crate::infra::lock::Error
}

/// Runtime settings loaded from environment variables.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Github API token for authenticated requests.
    pub github_token: Option<String>,
}

/// Severity level for a lint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Rule violation is an error.
    Error,
    /// Rule violation is a warning.
    Warn,
    /// Rule is disabled.
    Off,
}

/// Ignore target for a lint rule: action, workflow, and/or job.
/// All specified keys must match for the ignore to apply (intersection semantics).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IgnoreTarget {
    /// Action ID (e.g., "actions/checkout").
    pub action: Option<String>,
    /// Workflow file path (e.g., ".github/workflows/ci.yml").
    pub workflow: Option<String>,
    /// Job name within a workflow.
    pub job: Option<String>,
}

/// Configuration for a single lint rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Severity level (error, warn, off).
    pub level: Level,
    /// Targets to ignore (intersection semantics).
    #[serde(default)]
    pub ignore: Vec<IgnoreTarget>,
}

/// Configuration for all lint rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Lint {
    /// Per-rule configuration, keyed by rule name.
    #[serde(default)]
    pub rules: BTreeMap<String, Rule>,
}

impl Lint {
    /// Get the effective configuration for a rule, applying defaults if not explicitly configured.
    /// Each rule has its own default level; unconfigured rules use their defaults.
    #[must_use]
    pub fn get_rule(&self, name: &str, default_level: Level) -> Rule {
        self.rules.get(name).cloned().unwrap_or(Rule {
            level: default_level,
            ignore: Vec::new(),
        })
    }
}

/// All application configuration, loaded once at startup.
#[derive(Debug)]
pub struct Config {
    pub settings: Settings,
    pub manifest: Manifest,
    pub lock: Lock,
    pub lint_config: Lint,
    pub manifest_path: PathBuf,
    pub lock_path: PathBuf,
    /// Whether the manifest was auto-migrated from v1 format on load.
    pub manifest_migrated: bool,
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
    /// Returns [`ConfigError::Manifest`] if the manifest file cannot be parsed.
    /// Returns [`ConfigError::Lock`] if the lock file cannot be parsed.
    pub fn load(repo_root: &Path) -> Result<Self, Error> {
        let manifest_path = repo_root.join(".github").join(MANIFEST_FILE_NAME);
        let lock_path = repo_root.join(".github").join(LOCK_FILE_NAME);
        let parsed_manifest = crate::infra::manifest::parse(&manifest_path)?;
        let lock_store = LockStore::new(&lock_path);
        let lock = lock_store.load()?;
        Ok(Self {
            settings: Settings::from_env(),
            manifest: parsed_manifest.value,
            manifest_migrated: parsed_manifest.migrated,
            lock,
            lint_config: parse_lint_config(&manifest_path)?,
            manifest_path,
            lock_path,
        })
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{
        Config, Deserialize, IgnoreTarget, Level, Lint, Lock, Manifest, PathBuf, Rule, Settings,
    };

    #[derive(Deserialize)]
    struct LevelWrapper {
        level: Level,
    }

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
            lint_config: Lint::default(),
            manifest_path: PathBuf::from("gx.toml"),
            lock_path: PathBuf::from("gx.lock"),
            manifest_migrated: false,
        };
        assert_eq!(config.settings.github_token, Some("test_token".to_owned()));
    }

    #[test]
    fn app_config_load_returns_defaults_for_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        // No .github folder created — both files are missing
        let config = Config::load(dir.path()).unwrap();
        assert!(config.settings.github_token.is_none() || config.settings.github_token.is_some());
        assert!(config.manifest.specs().next().is_none());
        assert!(config.lock.is_empty());
        assert!(config.manifest_path.ends_with("gx.toml"));
        assert!(config.lock_path.ends_with("gx.lock"));
    }

    #[test]
    fn level_deserializes_from_string() {
        assert_eq!(
            toml::from_str::<LevelWrapper>("level = \"error\"")
                .unwrap()
                .level,
            Level::Error
        );
        assert_eq!(
            toml::from_str::<LevelWrapper>("level = \"warn\"")
                .unwrap()
                .level,
            Level::Warn
        );
        assert_eq!(
            toml::from_str::<LevelWrapper>("level = \"off\"")
                .unwrap()
                .level,
            Level::Off
        );
    }

    #[test]
    fn level_rejects_invalid_values() {
        assert!(toml::from_str::<LevelWrapper>("level = \"invalid\"").is_err());
    }

    #[test]
    fn rule_config_parses_with_level_only() {
        let toml_str = r#"
            level = "error"
        "#;
        let config: Rule = toml::from_str(toml_str).unwrap();
        assert_eq!(config.level, Level::Error);
        assert!(config.ignore.is_empty());
    }

    #[test]
    fn rule_config_parses_with_ignore_targets() {
        let toml_str = r#"
            level = "warn"
            ignore = [
                { action = "actions/checkout" },
                { workflow = ".github/workflows/ci.yml" },
            ]
        "#;
        let config: Rule = toml::from_str(toml_str).unwrap();
        assert_eq!(config.level, Level::Warn);
        assert_eq!(config.ignore.len(), 2);
        assert_eq!(config.ignore[0].action, Some("actions/checkout".to_owned()));
        assert_eq!(
            config.ignore[1].workflow,
            Some(".github/workflows/ci.yml".to_owned())
        );
    }

    #[test]
    fn ignore_target_with_intersection() {
        let toml_str = r#"
action = "actions/checkout"
workflow = ".github/workflows/ci.yml"
job = "build"
        "#;
        let target: IgnoreTarget = toml::from_str(toml_str).unwrap();
        assert_eq!(target.action, Some("actions/checkout".to_owned()));
        assert_eq!(target.workflow, Some(".github/workflows/ci.yml".to_owned()));
        assert_eq!(target.job, Some("build".to_owned()));
    }

    #[test]
    fn lint_config_parses_multiple_rules() {
        let toml_str = r#"
            [rules]
            sha-mismatch = { level = "error" }
            unpinned = { level = "error", ignore = [{ action = "actions/internal-tool" }] }
            stale-comment = { level = "off" }
        "#;
        let config: Lint = toml::from_str(toml_str).unwrap();
        assert_eq!(config.rules.len(), 3);
        assert_eq!(config.rules["sha-mismatch"].level, Level::Error);
        assert_eq!(config.rules["unpinned"].level, Level::Error);
        assert_eq!(config.rules["unpinned"].ignore.len(), 1);
        assert_eq!(config.rules["stale-comment"].level, Level::Off);
    }

    #[test]
    fn lint_config_default_is_empty() {
        let config = Lint::default();
        assert!(config.rules.is_empty());
    }

    #[test]
    fn lint_config_get_rule_uses_default_when_unconfigured() {
        let config = Lint::default();
        let rule = config.get_rule("sha-mismatch", Level::Error);
        assert_eq!(rule.level, Level::Error);
        assert!(rule.ignore.is_empty());
    }

    #[test]
    fn lint_config_get_rule_returns_configured_value() {
        let mut config = Lint::default();
        config.rules.insert(
            "unpinned".to_owned(),
            Rule {
                level: Level::Warn,
                ignore: vec![IgnoreTarget {
                    action: Some("actions/checkout".to_owned()),
                    workflow: None,
                    job: None,
                }],
            },
        );
        let rule = config.get_rule("unpinned", Level::Error);
        assert_eq!(rule.level, Level::Warn);
        assert_eq!(rule.ignore.len(), 1);
    }

    #[test]
    fn lint_config_get_rule_respects_off_level() {
        let mut config = Lint::default();
        config.rules.insert(
            "stale-comment".to_owned(),
            Rule {
                level: Level::Off,
                ignore: vec![],
            },
        );
        let rule = config.get_rule("stale-comment", Level::Warn);
        assert_eq!(rule.level, Level::Off);
    }
}

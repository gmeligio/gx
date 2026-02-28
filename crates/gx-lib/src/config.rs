use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::commands::app::AppError;
use crate::domain::{Lock, Manifest};
use crate::infrastructure::{
    LOCK_FILE_NAME, MANIFEST_FILE_NAME, parse_lint_config, parse_lock, parse_manifest,
};

/// Runtime settings loaded from environment variables.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Github API token for authenticated requests
    pub github_token: Option<String>,
}

/// Severity level for a lint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Rule violation is an error
    Error,
    /// Rule violation is a warning
    Warn,
    /// Rule is disabled
    Off,
}

/// Ignore target for a lint rule: action, workflow, and/or job.
/// All specified keys must match for the ignore to apply (intersection semantics).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IgnoreTarget {
    /// Action ID (e.g., "actions/checkout")
    pub action: Option<String>,
    /// Workflow file path (e.g., ".github/workflows/ci.yml")
    pub workflow: Option<String>,
    /// Job name within a workflow
    pub job: Option<String>,
}

/// Configuration for a single lint rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConfig {
    /// Severity level (error, warn, off)
    pub level: Level,
    /// Targets to ignore (intersection semantics)
    #[serde(default)]
    pub ignore: Vec<IgnoreTarget>,
}

/// Configuration for all lint rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LintConfig {
    /// Per-rule configuration, keyed by rule name
    #[serde(default)]
    pub rules: BTreeMap<String, RuleConfig>,
}

impl LintConfig {
    /// Get the effective configuration for a rule, applying defaults if not explicitly configured.
    /// Each rule has its own default level; unconfigured rules use their defaults.
    #[must_use]
    pub fn get_rule(&self, name: &str, default_level: Level) -> RuleConfig {
        self.rules.get(name).cloned().unwrap_or(RuleConfig {
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
    pub lint_config: LintConfig,
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
            lint_config: parse_lint_config(&manifest_path)?,
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
            lint_config: LintConfig::default(),
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

    #[test]
    fn level_deserializes_from_string() {
        #[derive(Deserialize)]
        struct Wrapper {
            #[allow(dead_code)]
            level: Level,
        }
        assert_eq!(
            toml::from_str::<Wrapper>("level = \"error\"")
                .unwrap()
                .level,
            Level::Error
        );
        assert_eq!(
            toml::from_str::<Wrapper>("level = \"warn\"").unwrap().level,
            Level::Warn
        );
        assert_eq!(
            toml::from_str::<Wrapper>("level = \"off\"").unwrap().level,
            Level::Off
        );
    }

    #[test]
    fn level_rejects_invalid_values() {
        #[derive(Deserialize)]
        struct Wrapper {
            #[allow(dead_code)]
            level: Level,
        }
        assert!(toml::from_str::<Wrapper>("level = \"invalid\"").is_err());
    }

    #[test]
    fn rule_config_parses_with_level_only() {
        let toml_str = r#"
            level = "error"
        "#;
        let config: RuleConfig = toml::from_str(toml_str).unwrap();
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
        let config: RuleConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.level, Level::Warn);
        assert_eq!(config.ignore.len(), 2);
        assert_eq!(
            config.ignore[0].action,
            Some("actions/checkout".to_string())
        );
        assert_eq!(
            config.ignore[1].workflow,
            Some(".github/workflows/ci.yml".to_string())
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
        assert_eq!(target.action, Some("actions/checkout".to_string()));
        assert_eq!(
            target.workflow,
            Some(".github/workflows/ci.yml".to_string())
        );
        assert_eq!(target.job, Some("build".to_string()));
    }

    #[test]
    fn lint_config_parses_multiple_rules() {
        let toml_str = r#"
            [rules]
            sha-mismatch = { level = "error" }
            unpinned = { level = "error", ignore = [{ action = "actions/internal-tool" }] }
            stale-comment = { level = "off" }
        "#;
        let config: LintConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.rules.len(), 3);
        assert_eq!(config.rules["sha-mismatch"].level, Level::Error);
        assert_eq!(config.rules["unpinned"].level, Level::Error);
        assert_eq!(config.rules["unpinned"].ignore.len(), 1);
        assert_eq!(config.rules["stale-comment"].level, Level::Off);
    }

    #[test]
    fn lint_config_default_is_empty() {
        let config = LintConfig::default();
        assert!(config.rules.is_empty());
    }

    #[test]
    fn lint_config_get_rule_uses_default_when_unconfigured() {
        let config = LintConfig::default();
        let rule = config.get_rule("sha-mismatch", Level::Error);
        assert_eq!(rule.level, Level::Error);
        assert!(rule.ignore.is_empty());
    }

    #[test]
    fn lint_config_get_rule_returns_configured_value() {
        let mut config = LintConfig::default();
        config.rules.insert(
            "unpinned".to_string(),
            RuleConfig {
                level: Level::Warn,
                ignore: vec![IgnoreTarget {
                    action: Some("actions/checkout".to_string()),
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
        let mut config = LintConfig::default();
        config.rules.insert(
            "stale-comment".to_string(),
            RuleConfig {
                level: Level::Off,
                ignore: vec![],
            },
        );
        let rule = config.get_rule("stale-comment", Level::Warn);
        assert_eq!(rule.level, Level::Off);
    }
}

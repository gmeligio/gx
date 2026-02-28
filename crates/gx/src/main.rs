use clap::{Parser, Subcommand};
use gx_lib::commands;
use gx_lib::commands::app::AppError;
use gx_lib::config::Config;
use gx_lib::infrastructure::{repo, repo::RepoError};
use log::{LevelFilter, info};
use std::io::Write;
use thiserror::Error;

/// Top-level error type for the gx CLI binary
#[derive(Debug, Error)]
enum GxError {
    /// The `--latest` flag was combined with an exact version pin.
    #[error(
        "--latest cannot be combined with an exact version pin (ACTION@VERSION). \
         Use --latest ACTION to upgrade to latest, or ACTION@VERSION to pin."
    )]
    LatestWithVersionPin,

    /// The action argument could not be parsed as ACTION@VERSION.
    #[error("invalid format: expected ACTION@VERSION (e.g., actions/checkout@v5), got: {input}")]
    InvalidActionFormat { input: String },

    /// Command orchestration failed.
    #[error(transparent)]
    App(#[from] AppError),

    /// Repository detection failed.
    #[error(transparent)]
    Repo(#[from] RepoError),

    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Parser)]
#[command(name = "gx")]
#[command(about = "CLI to manage Github Actions dependencies", long_about = None)]
#[command(version)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ensure the manifest and lock matches the workflow code: add missing actions, remove unused, update workflows
    Tidy,
    /// Create manifest and lock files from current workflows
    Init,
    /// Upgrade actions to newer versions
    Upgrade {
        /// Upgrade a specific action (e.g., actions/checkout or actions/checkout@v5)
        #[arg(value_name = "ACTION")]
        action: Option<String>,

        /// Upgrade to the absolute latest version, including major versions
        #[arg(long)]
        latest: bool,
    },
    /// Run lint checks on workflows
    Lint,
}

fn main() -> Result<(), GxError> {
    let cli = Cli::parse();

    init_logging(&cli);

    let cwd = std::env::current_dir()?;
    let repo_root = match repo::find_root(&cwd) {
        Ok(root) => root,
        Err(RepoError::GithubFolder) => {
            info!(".github folder not found. gx didn't modify any file.");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    let config = Config::load(&repo_root)?;

    match cli.command {
        Commands::Tidy => commands::app::tidy(&repo_root, config)?,
        Commands::Init => commands::app::init(&repo_root, config)?,
        Commands::Upgrade { action, latest } => {
            let request = resolve_upgrade_mode(action.as_deref(), latest)?;
            commands::app::upgrade(&repo_root, config, &request)?;
        }
        Commands::Lint => commands::app::lint(&repo_root, &config)?,
    }
    Ok(())
}

/// # Errors
///
/// Returns [`GxError::LatestWithVersionPin`] if `--latest` is combined with `ACTION@VERSION`.
/// Returns [`GxError::InvalidActionFormat`] if the action string cannot be parsed.
/// Propagates [`GxError::App`] from [`UpgradeRequest::new`].
fn resolve_upgrade_mode(
    action: Option<&str>,
    latest: bool,
) -> Result<commands::upgrade::UpgradeRequest, GxError> {
    use commands::upgrade::{UpgradeMode, UpgradeRequest, UpgradeScope};
    use gx_lib::domain::ActionId;

    match (action, latest) {
        // gx upgrade --latest
        (None, true) => {
            Ok(UpgradeRequest::new(UpgradeMode::Latest, UpgradeScope::All)
                .map_err(AppError::from)?)
        }

        // gx upgrade --latest actions/checkout
        (Some(action_str), true) => {
            // action_str is bare ACTION (no version)
            if action_str.contains('@') {
                return Err(GxError::LatestWithVersionPin);
            }
            let id = ActionId::from(action_str);
            Ok(
                UpgradeRequest::new(UpgradeMode::Latest, UpgradeScope::Single(id))
                    .map_err(AppError::from)?,
            )
        }

        // gx upgrade actions/checkout
        (Some(action_str), false) => {
            if action_str.contains('@') {
                // Bare ACTION@VERSION → Pinned mode
                let key = gx_lib::domain::LockKey::parse(action_str).ok_or_else(|| {
                    GxError::InvalidActionFormat {
                        input: action_str.to_string(),
                    }
                })?;
                Ok(UpgradeRequest::new(
                    UpgradeMode::Pinned(key.version),
                    UpgradeScope::Single(key.id),
                )
                .map_err(AppError::from)?)
            } else {
                // Bare ACTION → Safe mode, single action
                let id = ActionId::from(action_str);
                Ok(
                    UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::Single(id))
                        .map_err(AppError::from)?,
                )
            }
        }

        // gx upgrade
        (None, false) => Ok(
            UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All).map_err(AppError::from)?
        ),
    }
}

/// Initialize logging based on the verbosity level specified in the CLI
fn init_logging(cli: &Cli) {
    let mut builder = env_logger::builder();
    builder
        .filter_level(if cli.verbose {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        })
        .format(|buf, record| {
            let level = record.level();
            let style = &buf.default_level_style(level);
            writeln!(buf, "[{style}{level}{style:#}] {}", record.args())
        });

    if !cli.verbose {
        builder.format_timestamp(None);
    }

    builder.init();
}

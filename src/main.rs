#![allow(unused_crate_dependencies)]

use clap::{Parser, Subcommand};
use gx::commands;
use gx::commands::app::AppError;
use gx::commands::lint::LintError;
use gx::config::{Config, ConfigError};
use gx::infrastructure::{repo, repo::RepoError};
use gx::output::{
    LogFile, OutputLine, Printer, render_init, render_lint, render_tidy, render_upgrade,
};
use indicatif::ProgressBar;
use thiserror::Error;

/// Top-level error type for the gx CLI binary
#[derive(Debug, Error)]
enum GxError {
    #[error(
        "--latest cannot be combined with an exact version pin (ACTION@VERSION). \
         Use --latest ACTION to upgrade to latest, or ACTION@VERSION to pin."
    )]
    LatestWithVersionPin,

    #[error("invalid format: expected ACTION@VERSION (e.g., actions/checkout@v5), got: {input}")]
    InvalidActionFormat { input: String },

    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    App(#[from] AppError),

    #[error(transparent)]
    Repo(#[from] RepoError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Parser)]
#[command(name = "gx")]
#[command(about = "CLI to manage Github Actions dependencies", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ensure the manifest and lock matches the workflow code
    Tidy,
    /// Create manifest and lock files from current workflows
    Init,
    /// Upgrade actions to newer versions
    Upgrade {
        #[arg(value_name = "ACTION")]
        action: Option<String>,
        #[arg(long)]
        latest: bool,
    },
    /// Run lint checks on workflows
    Lint,
}

fn main() -> Result<(), GxError> {
    let cli = Cli::parse();

    let printer = Printer::new();
    let is_ci = printer.is_ci;

    let cmd_name = match &cli.command {
        Commands::Tidy => "tidy",
        Commands::Init => "init",
        Commands::Upgrade { .. } => "upgrade",
        Commands::Lint => "lint",
    };

    // Create log file for local runs (not CI)
    let mut log_file: Option<LogFile> = if is_ci {
        None
    } else {
        LogFile::new(cmd_name).ok()
    };

    if is_ci {
        printer.print_lines(&[OutputLine::CiNotice {
            message: "CI detected, running in verbose mode".to_string(),
        }]);
    }

    let cwd = std::env::current_dir()?;
    let repo_root = match repo::find_root(&cwd) {
        Ok(root) => root,
        Err(RepoError::GithubFolder) => {
            printer.print_lines(&[OutputLine::Summary {
                text: ".github folder not found. gx didn't modify any file.".to_string(),
            }]);
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    let config = Config::load(&repo_root)?;

    match cli.command {
        Commands::Tidy => {
            let spinner = printer.spinner("Running tidy...");
            let mut lf = log_file.take();
            let report = commands::app::tidy(
                &repo_root,
                config,
                make_cb(spinner.as_ref(), &mut lf, is_ci),
            )?;
            finish_spinner(spinner);
            let mut lines = render_tidy(&report);
            append_log_path(lf.as_ref(), &mut lines);
            printer.print_lines(&lines);
            log_file = lf;
        }
        Commands::Init => {
            let spinner = printer.spinner("Initializing...");
            let mut lf = log_file.take();
            let report = commands::app::init(
                &repo_root,
                config,
                make_cb(spinner.as_ref(), &mut lf, is_ci),
            )?;
            finish_spinner(spinner);
            let mut lines = render_init(&report);
            append_log_path(lf.as_ref(), &mut lines);
            printer.print_lines(&lines);
            log_file = lf;
        }
        Commands::Upgrade { action, latest } => {
            let request = resolve_upgrade_mode(action.as_deref(), latest)?;
            let spinner = printer.spinner("Checking actions...");
            let mut lf = log_file.take();
            let report = commands::app::upgrade(
                &repo_root,
                config,
                &request,
                make_cb(spinner.as_ref(), &mut lf, is_ci),
            )?;
            finish_spinner(spinner);
            let mut lines = render_upgrade(&report);
            append_log_path(lf.as_ref(), &mut lines);
            printer.print_lines(&lines);
            log_file = lf;
        }
        Commands::Lint => match commands::app::lint(&repo_root, &config) {
            Err(AppError::Lint(LintError::ViolationsFound { .. })) => {
                std::process::exit(1);
            }
            Err(err) => return Err(GxError::App(err)),
            Ok(report) => {
                let lines = render_lint(&report);
                printer.print_lines(&lines);
            }
        },
    }

    drop(log_file);
    Ok(())
}

/// Build a progress callback that updates the spinner, writes to the log file, and prints in CI.
fn make_cb<'a>(
    spinner: Option<&'a ProgressBar>,
    log_file: &'a mut Option<LogFile>,
    is_ci: bool,
) -> impl FnMut(&str) + 'a {
    move |msg: &str| {
        if let Some(pb) = spinner {
            pb.set_message(msg.to_string());
        }
        if let Some(lf) = log_file.as_mut() {
            lf.write(msg);
        }
        if is_ci {
            use std::time::{SystemTime, UNIX_EPOCH};
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let h = (secs / 3600) % 24;
            let m = (secs / 60) % 60;
            let s = secs % 60;
            println!(" [{h:02}:{m:02}:{s:02}] {msg}");
        }
    }
}

fn finish_spinner(spinner: Option<ProgressBar>) {
    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }
}

fn append_log_path(log_file: Option<&LogFile>, lines: &mut Vec<OutputLine>) {
    if let Some(lf) = log_file {
        lines.push(OutputLine::LogPath {
            path: lf.path().clone(),
        });
    }
}

/// # Errors
///
/// Returns errors for invalid upgrade mode combinations.
fn resolve_upgrade_mode(
    action: Option<&str>,
    latest: bool,
) -> Result<commands::upgrade::UpgradeRequest, GxError> {
    use commands::upgrade::{UpgradeMode, UpgradeRequest, UpgradeScope};
    use gx::domain::ActionId;

    match (action, latest) {
        (None, true) => {
            Ok(UpgradeRequest::new(UpgradeMode::Latest, UpgradeScope::All)
                .map_err(AppError::from)?)
        }
        (Some(action_str), true) => {
            if action_str.contains('@') {
                return Err(GxError::LatestWithVersionPin);
            }
            let id = ActionId::from(action_str);
            Ok(
                UpgradeRequest::new(UpgradeMode::Latest, UpgradeScope::Single(id))
                    .map_err(AppError::from)?,
            )
        }
        (Some(action_str), false) => {
            if action_str.contains('@') {
                let key = gx::domain::LockKey::parse(action_str).ok_or_else(|| {
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
                let id = ActionId::from(action_str);
                Ok(
                    UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::Single(id))
                        .map_err(AppError::from)?,
                )
            }
        }
        (None, false) => Ok(
            UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All).map_err(AppError::from)?
        ),
    }
}

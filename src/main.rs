#![expect(
    unused_crate_dependencies,
    reason = "dev-dependencies are only used in integration tests"
)]

use clap::{Parser, Subcommand};
use gx::command::{Command as _, CommandReport as _};
use gx::config::{Config, Error as ConfigError};
use gx::infra::{repo, repo::Error as RepoError};
use gx::init::Error as InitError;
use gx::lint::Error as LintError;
use gx::output::lines::Line as OutputLine;
use gx::output::log_file::LogFile;
use gx::output::printer::Printer;
use gx::tidy::RunError as TidyRunError;
use gx::upgrade::RunError as UpgradeRunError;
use gx::{init, lint, tidy, upgrade};
use indicatif::ProgressBar;
use thiserror::Error;

/// Top-level error type for the gx CLI binary.
#[derive(Debug, Error)]
enum GxError {
    /// Upgrade resolution failed.
    #[error(transparent)]
    Resolve(#[from] upgrade::cli::Error),

    /// Configuration loading failed.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// Init command failed.
    #[error(transparent)]
    Init(#[from] InitError),

    /// Tidy command failed.
    #[error(transparent)]
    Tidy(#[from] TidyRunError),

    /// Upgrade command failed.
    #[error(transparent)]
    Upgrade(#[from] UpgradeRunError),

    /// Lint command failed.
    #[error(transparent)]
    Lint(#[from] LintError),

    /// Repository detection failed.
    #[error(transparent)]
    Repo(#[from] RepoError),

    /// I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Parser)]
#[command(name = "gx")]
#[command(about = "CLI to manage Github Actions dependencies", long_about = None)]
#[command(version)]
/// CLI argument parser for the gx binary.
struct Cli {
    /// The subcommand to execute.
    #[command(subcommand)]
    command: Commands,
}

/// Available subcommands for the gx CLI.
#[derive(Subcommand)]
enum Commands {
    /// Ensure the manifest and lock matches the workflow code.
    Tidy,
    /// Create manifest and lock files from current workflows.
    Init,
    /// Upgrade actions to newer versions.
    Upgrade {
        /// Optional action identifier to upgrade (e.g., `actions/checkout`).
        #[arg(value_name = "ACTION")]
        action: Option<String>,
        /// Upgrade to the latest version instead of safe update.
        #[arg(long)]
        latest: bool,
    },
    /// Run lint checks on workflows.
    Lint,
}

/// Create a progress callback that updates the spinner, log file, and CI output.
fn make_cb<'cb>(
    spinner: Option<&'cb ProgressBar>,
    log_file: &'cb mut Option<LogFile>,
    is_ci: bool,
) -> impl FnMut(&str) + 'cb {
    move |msg: &str| {
        if let Some(pb) = spinner {
            pb.set_message(msg.to_owned());
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
            #[expect(
                clippy::print_stdout,
                reason = "CI verbose mode outputs directly to stdout"
            )]
            {
                println!(" [{h:02}:{m:02}:{s:02}] {msg}");
            }
        }
    }
}

/// Clear and finish the spinner if present.
fn finish_spinner(spinner: Option<ProgressBar>) {
    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }
}

/// Append the log file path to the output lines if a log file exists.
fn append_log_path(log_file: Option<&LogFile>, lines: &mut Vec<OutputLine>) {
    if let Some(lf) = log_file {
        lines.push(OutputLine::LogPath {
            path: lf.path().clone(),
        });
    }
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
            message: "CI detected, running in verbose mode".to_owned(),
        }]);
    }

    let cwd = std::env::current_dir()?;
    let repo_root = match repo::find_root(&cwd) {
        Ok(root) => root,
        Err(RepoError::GithubFolder) => {
            printer.print_lines(&[OutputLine::Summary {
                text: ".github folder not found. gx didn't modify any file.".to_owned(),
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
            let report = {
                let mut cb = make_cb(spinner.as_ref(), &mut lf, is_ci);
                tidy::Tidy.run(&repo_root, config, &mut cb)?
            };
            finish_spinner(spinner);
            let mut lines = report.render();
            append_log_path(lf.as_ref(), &mut lines);
            printer.print_lines(&lines);
            if report.exit_code() != 0 {
                std::process::exit(report.exit_code());
            }
            log_file = lf;
        }
        Commands::Init => {
            let spinner = printer.spinner("Initializing...");
            let mut lf = log_file.take();
            let report = {
                let mut cb = make_cb(spinner.as_ref(), &mut lf, is_ci);
                init::Init.run(&repo_root, config, &mut cb)?
            };
            finish_spinner(spinner);
            let mut lines = report.render();
            append_log_path(lf.as_ref(), &mut lines);
            printer.print_lines(&lines);
            if report.exit_code() != 0 {
                std::process::exit(report.exit_code());
            }
            log_file = lf;
        }
        Commands::Upgrade { action, latest } => {
            let request = upgrade::cli::resolve_upgrade_mode(action.as_deref(), latest)?;
            let spinner = printer.spinner("Checking actions...");
            let mut lf = log_file.take();
            let report = {
                let mut cb = make_cb(spinner.as_ref(), &mut lf, is_ci);
                upgrade::Upgrade { request }.run(&repo_root, config, &mut cb)?
            };
            finish_spinner(spinner);
            let mut lines = report.render();
            append_log_path(lf.as_ref(), &mut lines);
            printer.print_lines(&lines);
            if report.exit_code() != 0 {
                std::process::exit(report.exit_code());
            }
            log_file = lf;
        }
        Commands::Lint => {
            let spinner = printer.spinner("Linting...");
            let mut lf = log_file.take();
            let report = {
                let mut cb = make_cb(spinner.as_ref(), &mut lf, is_ci);
                lint::Lint.run(&repo_root, config, &mut cb)?
            };
            finish_spinner(spinner);
            let mut lines = report.render();
            append_log_path(lf.as_ref(), &mut lines);
            printer.print_lines(&lines);
            if report.exit_code() != 0 {
                std::process::exit(report.exit_code());
            }
            log_file = lf;
        }
    }

    drop(log_file);
    Ok(())
}

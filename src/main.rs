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
use gx::upgrade::command::RunError as UpgradeRunError;
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
        /// Emit the result as JSON on stdout (for scripting / unattended PRs).
        #[arg(long)]
        json: bool,
    },
    /// Run lint checks on workflows.
    ///
    /// Reports action-hygiene issues (sha-mismatch, unpinned, stale-comment,
    /// unsynced-manifest) and workflow-security issues (missing-permissions,
    /// excessive-permissions, dangerous-trigger, pr-head-checkout,
    /// missing-concurrency, unprotected-secrets). Configure per-rule severity
    /// and ignores under `[lint.rules]` in `.github/gx.toml`. See
    /// `docs/lint-rules.md`.
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

/// Run a command with the standard spinner → render → print flow shared by
/// tidy, init, and lint. Returns the log file so `main` keeps it alive; exits
/// the process directly on a non-zero report exit code.
fn run_reported<C>(
    printer: &Printer,
    repo_root: &std::path::Path,
    config: Config,
    is_ci: bool,
    log_file: Option<LogFile>,
    spinner_text: &str,
    command: &C,
) -> Result<Option<LogFile>, GxError>
where
    C: gx::command::Command,
    GxError: From<C::Error>,
{
    let spinner = printer.spinner(spinner_text);
    let mut lf = log_file;
    let report = {
        let mut cb = make_cb(spinner.as_ref(), &mut lf, is_ci);
        command.run(repo_root, config, &mut cb)?
    };
    finish_spinner(spinner);
    let mut lines = report.render();
    append_log_path(lf.as_ref(), &mut lines);
    printer.print_lines(&lines);
    if report.exit_code() != 0 {
        std::process::exit(report.exit_code());
    }
    Ok(lf)
}

/// Run the upgrade command, rendering either human output or, with `--json`, a
/// single JSON document on stdout. Returns the log file to keep it alive until
/// `main` drops it. Exits the process directly on a non-zero report exit code.
fn run_upgrade(
    printer: &Printer,
    repo_root: &std::path::Path,
    config: Config,
    log_file: Option<LogFile>,
    action: Option<&str>,
    latest: bool,
    json: bool,
) -> Result<Option<LogFile>, GxError> {
    let request = upgrade::cli::resolve_upgrade_mode(action, latest)?;
    // In JSON mode stdout must be a single JSON document, so suppress the spinner
    // and the local log file (their progress noise would corrupt it).
    let spinner = if json {
        None
    } else {
        printer.spinner("Checking actions...")
    };
    let mut lf = if json { None } else { log_file };
    let report = {
        let mut cb = make_cb(spinner.as_ref(), &mut lf, printer.is_ci && !json);
        upgrade::command::Upgrade { request }.run(repo_root, config, &mut cb)?
    };
    finish_spinner(spinner);

    if json {
        #[expect(clippy::print_stdout, reason = "JSON contract is written to stdout")]
        {
            println!("{}", report.to_json());
        }
    } else {
        let mut lines = report.render();
        append_log_path(lf.as_ref(), &mut lines);
        printer.print_lines(&lines);
    }

    if report.exit_code() != 0 {
        std::process::exit(report.exit_code());
    }

    Ok(lf)
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

    // `--json` turns stdout into a single machine-readable document, so every
    // human-facing line below (CI notice, log path, ".github not found") must be
    // suppressed for it.
    let json_mode = matches!(cli.command, Commands::Upgrade { json: true, .. });

    // Create log file for local runs (not CI)
    let mut log_file: Option<LogFile> = if is_ci || json_mode {
        None
    } else {
        LogFile::new(cmd_name).ok()
    };

    if is_ci && !json_mode {
        printer.print_lines(&[OutputLine::CiNotice {
            message: "CI detected, running in verbose mode".to_owned(),
        }]);
    }

    let cwd = std::env::current_dir()?;
    let repo_root = match repo::find_root(&cwd) {
        Ok(root) => root,
        Err(RepoError::GithubFolder) => {
            if json_mode {
                // Emit a valid, empty JSON document so a consumer's parser never breaks.
                #[expect(clippy::print_stdout, reason = "JSON contract is written to stdout")]
                {
                    println!("{}", upgrade::command::empty_json_report());
                }
            } else {
                printer.print_lines(&[OutputLine::Summary {
                    text: ".github folder not found. gx didn't modify any file.".to_owned(),
                }]);
            }
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    let config = Config::load(&repo_root)?;

    let lf = log_file.take();
    log_file = match cli.command {
        Commands::Tidy => run_reported(
            &printer,
            &repo_root,
            config,
            is_ci,
            lf,
            "Running tidy...",
            &tidy::Tidy,
        )?,
        Commands::Init => run_reported(
            &printer,
            &repo_root,
            config,
            is_ci,
            lf,
            "Initializing...",
            &init::Init,
        )?,
        Commands::Lint => run_reported(
            &printer,
            &repo_root,
            config,
            is_ci,
            lf,
            "Linting...",
            &lint::Lint,
        )?,
        Commands::Upgrade {
            action,
            latest,
            json,
        } => run_upgrade(
            &printer,
            &repo_root,
            config,
            lf,
            action.as_deref(),
            latest,
            json,
        )?,
    };

    drop(log_file);
    Ok(())
}

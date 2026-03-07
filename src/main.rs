#![allow(unused_crate_dependencies)]

use clap::{Parser, Subcommand};
use gx::config::{Config, ConfigError};
use gx::domain::{AppError, Command, CommandReport};
use gx::infra::{repo, repo::RepoError};
use gx::output::{LogFile, OutputLine, Printer};
use gx::{init, lint, tidy, upgrade};
use indicatif::ProgressBar;
use thiserror::Error;

/// Top-level error type for the gx CLI binary
#[derive(Debug, Error)]
enum GxError {
    #[error(transparent)]
    Resolve(#[from] upgrade::ResolveError),

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
            let request = upgrade::resolve_upgrade_mode(action.as_deref(), latest)?;
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

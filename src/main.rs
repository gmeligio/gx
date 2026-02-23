use anyhow::Result;
use clap::{Parser, Subcommand};
use gx::commands;
use gx::infrastructure::{LOCK_FILE_NAME, MANIFEST_FILE_NAME};
use gx::infrastructure::{repo, repo::RepoError};
use log::{LevelFilter, info};
use std::io::Write;

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
        /// Upgrade a specific action to a specific version (e.g., actions/checkout@v5)
        #[arg(value_name = "ACTION@VERSION")]
        action: Option<String>,

        /// Upgrade all actions to the absolute latest version, including major versions
        #[arg(long, conflicts_with = "action")]
        latest: bool,
    },
}

fn main() -> Result<()> {
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

    let manifest_path = repo_root.join(".github").join(MANIFEST_FILE_NAME);
    let lock_path = repo_root.join(".github").join(LOCK_FILE_NAME);

    match cli.command {
        Commands::Tidy => commands::app::tidy(&repo_root, &manifest_path, &lock_path),
        Commands::Init => commands::app::init(&repo_root, &manifest_path, &lock_path),
        Commands::Upgrade { action, latest } => {
            let mode = resolve_upgrade_mode(action, latest)?;
            commands::app::upgrade(&repo_root, &manifest_path, &lock_path, &mode)
        }
    }
}

fn resolve_upgrade_mode(
    action: Option<String>,
    latest: bool,
) -> Result<commands::upgrade::UpgradeMode> {
    if latest {
        Ok(commands::upgrade::UpgradeMode::Latest)
    } else if let Some(ref action_str) = action {
        let key = gx::domain::LockKey::parse(action_str).ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid format: expected ACTION@VERSION (e.g., actions/checkout@v5), got: {action_str}"
            )
        })?;
        Ok(commands::upgrade::UpgradeMode::Targeted(key.id, key.version))
    } else {
        Ok(commands::upgrade::UpgradeMode::Safe)
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

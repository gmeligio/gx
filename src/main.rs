use anyhow::Result;
use clap::{Parser, Subcommand};
use gx::commands;
use gx::infrastructure::{
    FileLock, FileWorkflowScanner, FileWorkflowUpdater, GithubRegistry, LOCK_FILE_NAME, MemoryLock,
};
use gx::infrastructure::{FileManifest, MANIFEST_FILE_NAME, MemoryManifest};
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

        /// Upgrade all actions to the absolute latest version, crossing major boundaries
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
        Commands::Tidy => {
            let registry = GithubRegistry::from_env()?;
            let updater = FileWorkflowUpdater::new(&repo_root);
            if manifest_path.exists() {
                let manifest = FileManifest::load_or_default(&manifest_path)?;
                let lock = FileLock::load_or_default(&lock_path)?;
                let scanner = FileWorkflowScanner::new(&repo_root);
                commands::tidy::run(&repo_root, manifest, lock, registry, &scanner, &updater)
            } else {
                let action_set = FileWorkflowScanner::new(&repo_root).scan_all()?;
                let manifest = MemoryManifest::from_workflows(&action_set);
                let lock = MemoryLock::default();
                let scanner = FileWorkflowScanner::new(&repo_root);
                commands::tidy::run(&repo_root, manifest, lock, registry, &scanner, &updater)
            }
        }
        Commands::Init => {
            if manifest_path.exists() {
                anyhow::bail!("Already initialized. Use `gx tidy` to update.");
            }
            info!("Reading actions from workflows into the manifest...");
            let registry = GithubRegistry::from_env()?;
            let manifest = FileManifest::load_or_default(&manifest_path)?;
            let lock = FileLock::load_or_default(&lock_path)?;
            let scanner = FileWorkflowScanner::new(&repo_root);
            let updater = FileWorkflowUpdater::new(&repo_root);
            commands::tidy::run(&repo_root, manifest, lock, registry, &scanner, &updater)
        }
        Commands::Upgrade { action, latest } => {
            let mode = if latest {
                commands::upgrade::UpgradeMode::Latest
            } else if let Some(ref action_str) = action {
                let key = gx::domain::LockKey::parse(action_str).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Invalid format: expected ACTION@VERSION (e.g., actions/checkout@v5), got: {action_str}"
                    )
                })?;
                commands::upgrade::UpgradeMode::Targeted(key.id, key.version)
            } else {
                commands::upgrade::UpgradeMode::Safe
            };

            let registry = GithubRegistry::from_env()?;
            let updater = FileWorkflowUpdater::new(&repo_root);
            if manifest_path.exists() {
                let manifest = FileManifest::load_or_default(&manifest_path)?;
                let lock = FileLock::load_or_default(&lock_path)?;
                commands::upgrade::run(&repo_root, manifest, lock, registry, &updater, &mode)
            } else {
                let action_set = FileWorkflowScanner::new(&repo_root).scan_all()?;
                let manifest = MemoryManifest::from_workflows(&action_set);
                let lock = MemoryLock::default();
                commands::upgrade::run(&repo_root, manifest, lock, registry, &updater, &mode)
            }
        }
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
